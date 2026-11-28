use std::time::Duration;

use domain::audit::OperationTag;
use domain::effect::{Call, Effect, EffectOutcome, ErrorClass, RetryPolicy};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const AGENT: &str = "agent";
pub const EXEC: &str = "exec";
pub const CALL: &str = "call";
pub const WAIT_HUMAN: &str = "wait.human";
pub const WAIT_POLL: &str = "wait.poll";
pub const EVENT: &str = "event";
pub const SLEEP: &str = "sleep";
pub const CHILD: &str = "child";
pub const STATUS: &str = "status.set";
pub const IMPEDIMENT: &str = "impediment.set";
pub const SCRIPT: &str = "script";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub seq: u64,
    pub kind: String,
    pub branch: String,
    #[serde(default)]
    pub step: Option<String>,
    #[serde(default)]
    pub attempt: u32,
    #[serde(default)]
    pub args: Value,
    #[serde(default)]
    pub connection: Option<String>,
    #[serde(default)]
    pub retry: Option<Value>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Signal {
    pub seq: u64,
    pub kind: String,
    pub branch: String,
    #[serde(default)]
    pub step: Option<String>,
    #[serde(default)]
    pub attempt: u32,
    #[serde(flatten)]
    pub data: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Activated {
    pub state: String,
    #[serde(default)]
    pub result: Value,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub error: Value,
    #[serde(default)]
    pub pending: Vec<Command>,
    #[serde(default)]
    pub cancel: Vec<u64>,
    #[serde(default)]
    pub signals: Vec<Signal>,
}

impl Command {
    pub fn text(&self, field: &str) -> String {
        self.args.get(field).and_then(Value::as_str).unwrap_or_default().to_owned()
    }

    pub fn works(&self) -> bool {
        matches!(self.kind.as_str(), AGENT | EXEC | CALL | SCRIPT)
    }

    pub fn call(&self) -> Call {
        let call = if self.kind == WAIT_POLL { &self.args["call"] } else { &self.args };
        let capability = call.get("capability").and_then(Value::as_str).unwrap_or_default().to_owned();
        let action = call.get("action").and_then(Value::as_str).unwrap_or_default();
        let action = if action.contains('.') { action.to_owned() } else { format!("{capability}.{action}") };
        Call {
            capability,
            action,
            connection: self.connection.clone(),
            args: call.get("args").cloned().unwrap_or_else(|| json!({})),
            operation: self.operation.as_ref().and_then(OperationTag::read),
        }
    }

    pub fn effect(&self) -> Effect {
        let millis = |value: &Value| Duration::from_millis(value.as_u64().unwrap_or(0));
        match self.kind.as_str() {
            AGENT => Effect::AgentRun { agent: self.text("name"), input: self.args.clone() },
            EXEC => Effect::Exec {
                command: self.text("command"),
                cwd: self.args.get("cwd").and_then(Value::as_str).map(str::to_owned),
            },
            WAIT_HUMAN => Effect::WaitHuman { view: self.args["view"].clone() },
            WAIT_POLL => Effect::WaitPoll { call: self.call(), every: millis(&self.args["every"]) },
            EVENT => Effect::WaitEvent {
                kind: self.text("kind"),
                filter: self.args["filter"].clone(),
                timeout: self.timeout.map(Duration::from_millis),
            },
            SLEEP => Effect::Sleep { duration: millis(&self.args["ms"]) },
            CHILD => Effect::Child { workflow: self.text("workflow"), input: self.args["input"].clone() },
            STATUS => Effect::StatusSet { name: self.text("name") },
            IMPEDIMENT => Effect::ImpedimentSet {
                on: self.args["on"].as_bool().unwrap_or(false),
                reason: self.args.get("reason").and_then(Value::as_str).map(str::to_owned),
            },
            SCRIPT => Effect::Script { name: self.text("name"), input: self.args["input"].clone() },
            _ => Effect::ConnectorCall(self.call()),
        }
    }

    pub fn retry(&self, default: &RetryPolicy) -> RetryPolicy {
        let Some(given) = self.retry.as_ref().filter(|given| given.is_object()) else { return default.clone() };
        let millis = |name: &str, fallback: Duration| given[name].as_u64().map_or(fallback, Duration::from_millis);
        RetryPolicy {
            max: given["max"].as_u64().and_then(|max| u32::try_from(max).ok()).unwrap_or(default.max),
            initial: millis("initial", default.initial),
            max_interval: millis("maxInterval", default.max_interval),
            multiplier: given["multiplier"].as_f64().unwrap_or(default.multiplier),
            jitter: given["jitter"].as_f64().unwrap_or(default.jitter),
        }
    }
}

pub fn kind_of(effect: &Effect) -> &'static str {
    match effect {
        Effect::AgentRun { .. } => AGENT,
        Effect::Exec { .. } => EXEC,
        Effect::ConnectorCall(_) => CALL,
        Effect::WaitHuman { .. } => WAIT_HUMAN,
        Effect::WaitPoll { .. } => WAIT_POLL,
        Effect::WaitEvent { .. } => EVENT,
        Effect::Sleep { .. } => SLEEP,
        Effect::Child { .. } => CHILD,
        Effect::StatusSet { .. } => STATUS,
        Effect::ImpedimentSet { .. } => IMPEDIMENT,
        Effect::Script { .. } => SCRIPT,
    }
}

pub fn class_name(class: &ErrorClass) -> &'static str {
    match class {
        ErrorClass::Transient => "transient",
        ErrorClass::RateLimited { .. } => "rate_limited",
        ErrorClass::Auth => "auth",
        ErrorClass::Conflict => "conflict",
        ErrorClass::Permanent => "permanent",
    }
}

pub fn journal_entry(seq: u64, kind: &str, outcome: &EffectOutcome, at: i64) -> Value {
    let mut entry = json!({ "seq": seq, "kind": kind, "at": at });
    match outcome {
        EffectOutcome::Ok { value } => entry["ok"] = value.clone(),
        EffectOutcome::Failed { class, message } => {
            entry["error"] = json!({ "kind": class_name(class), "message": message });
        }
        EffectOutcome::TimedOut => entry["error"] = json!({ "kind": "timeout", "message": "timed out" }),
        EffectOutcome::Cancelled => entry["cancelled"] = Value::Bool(true),
    }
    entry
}
