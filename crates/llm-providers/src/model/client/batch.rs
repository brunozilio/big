use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use llm_providers::batch::{self, Windows};
use llm_providers::{Call, Config, Provider};
use serde_json::Value;
use tokio::sync::oneshot;

use super::super::http::{Http, Outgoing};
use super::super::{CancellationToken, ModelError};

type Answered = Result<String, ModelError>;

struct Waiting {
    body: Value,
    answer: oneshot::Sender<Answered>,
}

static QUEUE: Mutex<Option<HashMap<String, Vec<Waiting>>>> = Mutex::new(None);

#[derive(Clone)]
pub(super) struct Job {
    pub provider: &'static dyn Provider,
    pub http: Arc<dyn Http>,
    pub config: Config,
    pub windows: Windows,
}

const SLACK: Duration = Duration::from_secs(60);
const RESULTS_LIMIT: usize = 64 << 20;
const STATE_LIMIT: usize = 1 << 20;

fn queued(key: &str, waiting: Waiting) -> bool {
    let mut queue = QUEUE.lock().unwrap_or_else(PoisonError::into_inner);
    let calls = queue.get_or_insert_with(HashMap::new).entry(key.to_owned()).or_default();
    calls.push(waiting);
    calls.len() == 1
}

fn drained(key: &str) -> Vec<Waiting> {
    let mut queue = QUEUE.lock().unwrap_or_else(PoisonError::into_inner);
    queue.as_mut().and_then(|queue| queue.remove(key)).unwrap_or_default()
}

pub(super) async fn answer(job: Job, key: String, body: Value, cancel: &CancellationToken) -> Answered {
    let (sender, receiver) = oneshot::channel();
    let most = job.windows.gather + job.windows.wait + SLACK;
    if queued(&key, Waiting { body, answer: sender }) {
        let runtime = platform::runtime::shared().map_err(|error| ModelError::Network(error.to_string()))?;
        runtime.spawn(async move {
            tokio::time::sleep(job.windows.gather).await;
            settle(&job, drained(&key)).await;
        });
    }
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(ModelError::Cancelled),
        () = tokio::time::sleep(most) => Err(ModelError::Provider("the batch gave no answer in time".into())),
        answered = receiver => answered.unwrap_or_else(|_| Err(ModelError::Provider("the batch was dropped".into()))),
    }
}

async fn settle(job: &Job, calls: Vec<Waiting>) {
    if calls.is_empty() {
        return;
    }
    let requests: Vec<(String, Value)> =
        calls.iter().enumerate().map(|(at, waiting)| (format!("call-{at}"), waiting.body.clone())).collect();
    let outcome = run(job, &requests).await;
    for ((id, _), waiting) in requests.iter().zip(calls) {
        let answered = match &outcome {
            Ok(answers) => answers.get(id).cloned().unwrap_or_else(|| Err(missing(id))),
            Err(error) => Err(error.clone()),
        };
        let _ = waiting.answer.send(answered);
    }
}

fn missing(id: &str) -> ModelError {
    ModelError::Provider(format!("the batch has no answer for {id}"))
}

async fn run(job: &Job, requests: &[(String, Value)]) -> Result<HashMap<String, Answered>, ModelError> {
    let (provider, config) = (job.provider, &job.config);
    let started = Instant::now();
    let submitted = json_of(job, &batch::submit(provider, config, requests)?).await?;
    let mut state = batch::state(provider, &submitted)?;
    while !state.done {
        if started.elapsed() + job.windows.poll > job.windows.wait {
            if let Some(call) = batch::cancel(provider, config, &state.id) {
                let _ = fetch(job, &call, STATE_LIMIT).await;
            }
            return Err(ModelError::Provider("the batch did not end in time".into()));
        }
        tokio::time::sleep(job.windows.poll).await;
        let polled = json_of(job, &batch::poll(provider, config, &state.id)?).await?;
        state = batch::state(provider, &polled)?;
    }
    let url = state.results.ok_or_else(|| ModelError::Provider("the ended batch has no results".into()))?;
    let text = fetch(job, &batch::results(provider, config, &url)?, RESULTS_LIMIT).await?;
    let answers = batch::answers(provider, &text)?;
    Ok(answers
        .into_iter()
        .map(|answer| {
            let answered = match (answer.stream, answer.error) {
                (Some(stream), None) => Ok(stream),
                (_, error) => Err(ModelError::Provider(error.unwrap_or_else(|| "the batched call failed".into()))),
            };
            (answer.id, answered)
        })
        .collect())
}

async fn json_of(job: &Job, call: &Call) -> Result<Value, ModelError> {
    let text = fetch(job, call, STATE_LIMIT).await?;
    serde_json::from_str(&text)
        .map_err(|_| ModelError::Provider("the batch answered with something other than JSON".into()))
}

async fn fetch(job: &Job, call: &Call, limit: usize) -> Result<String, ModelError> {
    let never = CancellationToken::new();
    let method = if call.body.is_some() { "POST" } else { "GET" };
    let out = Outgoing { method, url: &call.url, headers: &call.headers, body: call.body.as_deref(), streaming: false };
    let response = job.http.send(out, &never).await?;
    super::transport::status(response.status)?;
    response.body.text(&never, limit).await
}
