use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_engine::EventSink;
use domain::workflow::{ItemId, WorkflowEvent};
use domain::{ChoreAnswer, EngineEvent, ModelOption, PromptRequest, RunId, SessionCommand};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::shared::Shared;

const WAIT: Duration = Duration::from_secs(60);

fn chosen(shared: &Shared, wanted: Option<&str>) -> ModelOption {
    let models = shared.config.engine.models();
    let exact =
        wanted.filter(|wanted| *wanted != "auto").and_then(|wanted| models.iter().find(|model| model.id == wanted));
    let near = || wanted.and_then(|wanted| models.iter().find(|model| model.id.starts_with(wanted)));
    exact.or_else(near).or(models.first()).cloned().unwrap_or_else(|| {
        let id = wanted.unwrap_or("auto").to_owned();
        ModelOption { id: id.clone(), label: id, provider: Default::default(), window: None, capabilities: None }
    })
}

async fn asked(shared: &Shared, text: String, cwd: PathBuf, wanted: Option<&str>) -> Option<String> {
    let mut request = PromptRequest::plain(cwd, text, chosen(shared, wanted));
    request.session.command = Some(SessionCommand::Title);
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let sink: Arc<dyn EventSink> = Arc::new(move |_run: RunId, event: EngineEvent| {
        let _ = sender.send(event);
    });
    let _handle = shared.config.engine.start(RunId::new(), &request, sink).ok()?;
    let waited = tokio::time::timeout(WAIT, async {
        while let Some(event) = receiver.recv().await {
            match event {
                EngineEvent::Chore(ChoreAnswer::Title(title)) => return Some(title),
                EngineEvent::Completed { .. }
                | EngineEvent::Failed(_)
                | EngineEvent::Failure(_)
                | EngineEvent::Cancelled => {
                    return None;
                }
                _ => {}
            }
        }
        None
    });
    waited.await.ok().flatten().filter(|title| !title.trim().is_empty())
}

pub fn name(shared: &Arc<Shared>, item: ItemId, text: String, cwd: PathBuf, wanted: Option<String>) {
    let shared = Arc::clone(shared);
    let held = Arc::clone(&shared);
    held.spawn(async move {
        let Some(title) = asked(&shared, text, cwd, wanted.as_deref()).await else { return };
        if let Err(error) = shared.append(&item, &[WorkflowEvent::Named { title }]) {
            tracing::warn!(%error, "the item's title could not be kept");
        }
    });
}

pub fn titled(input: &Value) -> bool {
    let issue = input["issue"].as_str().is_some_and(|issue| !issue.is_empty());
    let sources = input["sources"].as_array().map(Vec::as_slice).unwrap_or_default();
    issue || sources.iter().any(|source| matches!(source["kind"].as_str(), Some("issue" | "pull_request")))
}
