use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use crate::error::{ActionError, ErrorClass};

pub const WRAPPER: &str = "@connector";

pub fn wrapper(entry: &str) -> String {
    let quoted = serde_json::to_string(entry).unwrap_or_default();
    format!(
        r#"import * as provider from {quoted};
const found = (name) => name.split(".").reduce((value, key) => (value == null ? undefined : value[key]), provider);
const classed = (error) => {{
  if (error && typeof error === "object" && error.class) return {{ class: error.class, message: String(error.message ?? "") }};
  const message = String(error?.message ?? error);
  try {{ const parsed = JSON.parse(message); if (parsed && parsed.class) return parsed; }} catch (_) {{}}
  return {{ class: {{ kind: "permanent" }}, message }};
}};
export function kind(name) {{
  return typeof found(name);
}}
export async function invoke(name, input, ctx) {{
  const target = found(name);
  try {{
    if (typeof target !== "function") return {{ ok: target === undefined ? null : target, plain: true }};
    const value = await target(input, ctx);
    return {{ ok: value === undefined ? null : value }};
  }} catch (error) {{
    return {{ error: classed(error) }};
  }}
}}
"#
    )
}

pub fn error_of(value: &Value) -> ActionError {
    let class = serde_json::from_value::<ErrorClass>(value["class"].clone()).unwrap_or(ErrorClass::Permanent);
    ActionError::new(class, value["message"].as_str().unwrap_or_default())
}

pub fn encoded(error: &ActionError) -> String {
    serde_json::to_string(error).unwrap_or_else(|_| error.message.clone())
}

pub fn outcome(answer: Result<Value, String>) -> Result<Value, ActionError> {
    let answer = answer.map_err(ActionError::permanent)?;
    if let Some(error) = answer.get("error") {
        return Err(error_of(error));
    }
    Ok(answer.get("ok").cloned().unwrap_or(Value::Null))
}

pub trait Answers: Send + Sync + std::fmt::Debug {
    fn now(&self, function: &str, arguments: &Value) -> Option<Result<Value, String>>;
}

#[derive(Debug, Clone)]
pub struct Script {
    pub modules: Arc<Vec<(String, String)>>,
    pub data: Value,
}

impl Script {
    pub fn kind(&self, name: &str) -> String {
        let stop = Arc::new(AtomicBool::new(false));
        let host: Arc<dyn script::Host> = Arc::new(super::host::Local);
        let limit = script::Limit::Busy(script::MOST_BUSY);
        let found = script::run_modules_in(&self.modules, WRAPPER, "kind", &[json!(name)], &self.data, host, stop, limit);
        found.ok().and_then(|kind| kind.as_str().map(str::to_owned)).unwrap_or_default()
    }

    pub fn invoke_here(&self, name: &str, input: &Value, host: Arc<dyn script::Host>) -> Result<Value, ActionError> {
        let stop = Arc::new(AtomicBool::new(false));
        let arguments = [json!(name), input.clone()];
        outcome(script::run_modules_in(
            &self.modules,
            WRAPPER,
            "invoke",
            &arguments,
            &self.data,
            host,
            stop,
            script::Limit::Busy(script::MOST_BUSY),
        ))
    }
}

struct Call {
    function: String,
    arguments: Value,
    reply: std::sync::mpsc::Sender<Result<Value, String>>,
}

struct Remote {
    calls: mpsc::UnboundedSender<Call>,
    answers: Arc<dyn Answers>,
}

impl script::Host for Remote {
    fn call(&self, function: &str, arguments: Value) -> Result<Value, String> {
        if let Some(answer) = self.answers.now(function, &arguments) {
            return answer;
        }
        let (reply, answer) = std::sync::mpsc::channel();
        let call = Call { function: function.to_owned(), arguments, reply };
        self.calls.send(call).map_err(|_| "the app stopped listening".to_owned())?;
        answer.recv().map_err(|_| "the app stopped answering".to_owned())?
    }
}

struct Stopper(Arc<AtomicBool>);

impl Drop for Stopper {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub trait Waits {
    fn answer(&self, function: &str, arguments: Value) -> impl Future<Output = Result<Value, String>> + Send;
}

pub async fn invoke<W: Waits + Sync>(
    script: &Script,
    answers: Arc<dyn Answers>,
    waits: &W,
    name: &str,
    input: Value,
) -> Result<Value, ActionError> {
    let (calls, mut requests) = mpsc::unbounded_channel();
    let (finished, result) = oneshot::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let _stopper = Stopper(Arc::clone(&stop));
    let host: Arc<dyn script::Host> = Arc::new(Remote { calls, answers });
    let modules = Arc::clone(&script.modules);
    let data = script.data.clone();
    let arguments = [json!(name), input];
    std::thread::spawn(move || {
        let limit = script::Limit::Busy(script::MOST_BUSY);
        let ran = script::run_modules_in(&modules, WRAPPER, "invoke", &arguments, &data, host, stop, limit);
        let _ = finished.send(ran);
    });
    while let Some(call) = requests.recv().await {
        let answer = waits.answer(&call.function, call.arguments).await;
        let _ = call.reply.send(answer);
    }
    outcome(result.await.unwrap_or_else(|_| Err("the script stopped without an answer".to_owned())))
}
