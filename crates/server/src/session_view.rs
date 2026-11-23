use domain::conversation::{Entry, Outcome, SessionStatus, ToolState, Transcript};
use domain::trace::Record;
use domain::{AgentAsk, AskKind, Decision, PermissionMode};
use serde_json::{Value, json};

use crate::marks::iso;

#[derive(Debug, Default)]
pub struct Shown {
    pub transcript: Transcript,
    pub times: Vec<i64>,
    pub mode: String,
    pub model: String,
}

fn at_of(record: &Record) -> Option<i64> {
    let at = match record {
        Record::Prompted { at_ms, .. } | Record::Event { at_ms, .. } | Record::Called { at_ms, .. } => *at_ms,
        Record::Opened { at_ms, .. } => *at_ms,
        _ => return None,
    };
    i64::try_from(at).ok()
}

pub fn mode_named(permission: &str) -> &'static str {
    match permission.to_ascii_lowercase().as_str() {
        "plan" => "plan",
        "bypass" => "bypass",
        "ask" => "ask",
        _ => "accept_edits",
    }
}

impl Shown {
    pub fn of(records: &[Record]) -> Shown {
        let mut shown = Shown { mode: "accept_edits".to_owned(), ..Shown::default() };
        let (mut start, mut last) = (0, records.iter().find_map(at_of).unwrap_or(0));
        for (at, record) in records.iter().enumerate() {
            if let Record::Opened { run, .. } = record {
                shown.mode = mode_named(&run.permission).to_owned();
                shown.model.clone_from(&run.model);
            }
            if matches!(record, Record::Shown { .. }) {
                continue;
            }
            last = at_of(record).unwrap_or(last);
            shown.transcript.follow(&records[start..=at]);
            start = at + 1;
            shown.stamp(last);
        }
        shown
    }

    pub fn stamp(&mut self, at: i64) {
        let count = self.transcript.entries.len();
        self.times.truncate(count);
        while self.times.len() < count {
            self.times.push(at);
        }
    }
}

fn tool_kind(name: &str) -> &'static str {
    match name {
        "Bash" | "BashOutput" | "KillShell" => "command",
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => "edit",
        "Read" => "read",
        "Grep" | "Glob" | "LS" | "WebSearch" => "search",
        "Agent" | "Task" => "subagent",
        _ => "call",
    }
}

fn tool_state(state: &ToolState) -> &'static str {
    match state {
        ToolState::Running => "running",
        ToolState::Done => "ok",
        ToolState::Failed => "error",
    }
}

fn call(entry: &Entry) -> Option<Value> {
    match entry {
        Entry::Tool { id, name, summary, state, output } => Some(json!({
            "id": id, "kind": tool_kind(name), "title": if summary.is_empty() { name } else { summary },
            "tag": name, "state": tool_state(state), "output": output, "durationMs": 0,
        })),
        Entry::Agent { index, kind, summary, done, steps } => Some(json!({
            "id": format!("agent-{index}"), "kind": "subagent", "title": summary, "tag": kind,
            "state": match done { None => "running", Some(true) => "ok", Some(false) => "error" }, "durationMs": 0,
            "steps": steps,
        })),
        _ => None,
    }
}

fn plan(text: &str, resolution: &str) -> Value {
    json!({ "type": "plan", "text": text, "resolution": resolution })
}

fn single(entry: &Entry, attached: usize) -> Option<Value> {
    let event = match entry {
        Entry::User(text) => json!({
            "type": "user", "author": { "id": "you", "name": "Você", "avatar": null, "you": true },
            "text": text, "attachments": attached,
        }),
        Entry::Thinking { text, done } => json!({
            "type": "thinking", "seconds": 0, "steps": [{ "text": text, "done": done }], "live": !done,
        }),
        Entry::Assistant(text) => json!({ "type": "assistant", "text": text }),
        Entry::Plan { plan: text, outcome } => plan(
            text,
            match outcome {
                Outcome::Waiting => "pending",
                Outcome::Approved(_) => "approved",
                Outcome::KeptPlanning => "revised",
            },
        ),
        Entry::Refused { tool, reason } => {
            json!({ "type": "status", "status": "error", "text": format!("{tool}: {reason}") })
        }
        Entry::Error(text) => json!({ "type": "status", "status": "error", "text": text }),
        Entry::Notice(text) => json!({ "type": "status", "status": "info", "text": text }),
        Entry::Interrupted => json!({ "type": "status", "status": "info", "text": "Interrompido" }),
        _ => return None,
    };
    Some(event)
}

fn stamped(mut event: Value, id: String, at: i64) -> Value {
    event["id"] = json!(id);
    event["at"] = json!(iso(at));
    event
}

pub fn ask_event(ask: &AgentAsk, at: i64) -> Value {
    let event = match &ask.kind {
        AskKind::Permission { tool, subject, .. } => json!({
            "type": "permission", "tool": tool, "detail": subject, "risk": "medium", "resolution": "pending",
        }),
        AskKind::Questions(questions) => {
            let asked: Vec<Value> = questions
                .iter()
                .map(|question| {
                    let options: Vec<Value> = question
                        .options
                        .iter()
                        .map(|choice| json!({ "label": choice.label, "description": choice.description }))
                        .collect();
                    json!({ "question": question.text, "header": question.header, "multiSelect": question.multi, "options": options })
                })
                .collect();
            json!({ "type": "ask", "questions": asked, "answers": null })
        }
        AskKind::Plan { plan: text } => plan(text, "pending"),
    };
    stamped(event, ask.id.0.clone(), at)
}

pub fn events(shown: &Shown, pending: Option<&AgentAsk>) -> Vec<Value> {
    let entries = &shown.transcript.entries;
    let time = |at: usize| shown.times.get(at).copied().unwrap_or(0);
    let mut listed: Vec<Value> = Vec::new();
    let mut tools: Option<(usize, Vec<Value>)> = None;
    let mut plan_taken = false;
    let close = |tools: &mut Option<(usize, Vec<Value>)>, listed: &mut Vec<Value>| {
        if let Some((first, calls)) = tools.take() {
            let live = calls.iter().any(|call| call["state"] == "running");
            let event = json!({ "type": "tools", "calls": calls, "seconds": 0, "live": live });
            listed.push(stamped(event, format!("e{first}"), time(first)));
        }
    };
    for (at, entry) in entries.iter().enumerate() {
        if let Some(found) = call(entry) {
            tools.get_or_insert_with(|| (at, Vec::new())).1.push(found);
            continue;
        }
        close(&mut tools, &mut listed);
        let attached = match at.checked_sub(1).map(|before| &entries[before]) {
            Some(Entry::Attached { names }) => names.len(),
            _ => 0,
        };
        let Some(event) = single(entry, attached) else { continue };
        let waiting_plan = matches!(entry, Entry::Plan { outcome: Outcome::Waiting, .. });
        let id = match pending {
            Some(ask) if waiting_plan && matches!(ask.kind, AskKind::Plan { .. }) => {
                plan_taken = true;
                ask.id.0.clone()
            }
            _ => format!("e{at}"),
        };
        listed.push(stamped(event, id, time(at)));
    }
    close(&mut tools, &mut listed);
    if let Some(ask) = pending.filter(|_| !plan_taken) {
        listed.push(ask_event(ask, shown.times.last().copied().unwrap_or(0)));
    }
    listed
}

pub fn work(transcript: &Transcript) -> Value {
    let tasks: Vec<Value> = transcript
        .tasks
        .iter()
        .map(|task| json!({ "id": task.id, "subject": task.subject, "status": task.status }))
        .collect();
    json!({ "background": transcript.background, "tasks": tasks })
}

pub fn status(transcript: &Transcript, pending: bool) -> &'static str {
    if pending {
        return "waiting";
    }
    match transcript.status() {
        SessionStatus::Running | SessionStatus::Thinking => "running",
        SessionStatus::NeedsYou => "waiting",
        _ => "idle",
    }
}

fn answers(questions: &[domain::Question], given: &Value) -> Vec<Vec<usize>> {
    let given: Vec<&str> =
        given.as_array().map(|all| all.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    questions
        .iter()
        .enumerate()
        .map(|(at, question)| {
            let said = given.get(at).copied().unwrap_or_default();
            let labels: Vec<&str> = said.split(", ").collect();
            question
                .options
                .iter()
                .enumerate()
                .filter(|(_, choice)| labels.contains(&choice.label.as_str()))
                .map(|(n, _)| n)
                .collect()
        })
        .collect()
}

pub fn decision(kind: &AskKind, response: &Value) -> Option<Decision> {
    match (response["kind"].as_str()?, kind) {
        ("ask", AskKind::Questions(questions)) => Some(Decision::Answer(answers(questions, &response["answers"]))),
        ("permission", _) => match response["decision"].as_str()? {
            "allowed" => Some(Decision::Allow),
            "always" => Some(Decision::AllowAlways),
            "denied" => Some(Decision::Deny),
            _ => None,
        },
        ("plan", _) => match response["decision"].as_str()? {
            "approve" => Some(Decision::Approve(PermissionMode::AcceptEdits)),
            "revise" => Some(match response["feedback"].as_str().filter(|text| !text.trim().is_empty()) {
                Some(feedback) => Decision::DenyWith(feedback.to_owned()),
                None => Decision::Deny,
            }),
            _ => None,
        },
        _ => None,
    }
}

pub fn resolved(mut event: Value, response: &Value) -> Value {
    match event["type"].as_str() {
        Some("ask") => event["answers"] = response["answers"].clone(),
        Some("permission") => event["resolution"] = response["decision"].clone(),
        Some("plan") => {
            let approved = response["decision"] == "approve";
            event["resolution"] = json!(if approved { "approved" } else { "revised" });
            if let Some(feedback) = response["feedback"].as_str() {
                event["feedback"] = json!(feedback);
            }
        }
        _ => {}
    }
    event
}

#[cfg(test)]
#[path = "session_view_tests.rs"]
mod tests;
