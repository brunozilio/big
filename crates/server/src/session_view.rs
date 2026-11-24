use domain::conversation::{Entry, Outcome, SessionStatus, ToolState, Transcript};
use domain::trace::Record;
use domain::{AgentAsk, AskKind, Decision, PermissionMode, ToolName};
use serde_json::{Value, json};

use crate::marks::iso;

#[derive(Debug, Default)]
pub struct Shown {
    pub transcript: Transcript,
    pub times: Vec<i64>,
    pub ended: Vec<Option<i64>>,
    pub running: Vec<usize>,
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

pub fn free_mode(agent: Option<&domain::MainAgentSpec>, shown: &Shown) -> Option<PermissionMode> {
    (agent.is_none() && shown.mode == "plan").then_some(PermissionMode::Plan)
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
        self.settle(at);
    }

    fn settle(&mut self, at: i64) {
        let entries = &self.transcript.entries;
        let seen = self.ended.len().min(entries.len());
        self.ended.truncate(entries.len());
        self.running.retain(|index| *index < entries.len());
        for index in seen..entries.len() {
            self.ended.push(None);
            self.running.push(index);
        }
        let ended = &mut self.ended;
        self.running.retain(|index| {
            let still = entries.get(*index).is_some_and(running);
            if !still && let Some(slot) = ended.get_mut(*index) {
                *slot = Some(at);
            }
            still
        });
    }

    pub fn took(&self, index: usize) -> i64 {
        let started = self.times.get(index).copied().unwrap_or(0);
        self.ended.get(index).copied().flatten().map_or(0, |ended| (ended - started).max(0))
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

fn running(entry: &Entry) -> bool {
    match entry {
        Entry::Tool { state, .. } => matches!(state, ToolState::Running),
        Entry::Agent { done, .. } => done.is_none(),
        _ => false,
    }
}

fn call(entry: &Entry, took: i64) -> Option<Value> {
    match entry {
        Entry::Tool { id, name, summary, state, output, input } => Some(json!({
            "id": id, "kind": tool_kind(name), "title": if summary.is_empty() { name } else { summary },
            "tag": name, "state": tool_state(state), "output": output, "input": input, "durationMs": took,
        })),
        Entry::Agent { index, kind, summary, done, steps } => Some(json!({
            "id": format!("agent-{index}"), "kind": "subagent", "title": summary, "tag": kind,
            "state": match done { None => "running", Some(true) => "ok", Some(false) => "error" }, "durationMs": took,
            "steps": steps,
        })),
        _ => None,
    }
}

fn into_script(calls: &mut [Value], entry: &Entry) -> bool {
    let Entry::Agent { kind, summary, steps, .. } = entry else { return false };
    if kind != ToolName::Repl.as_str() {
        return false;
    }
    let script = calls
        .iter_mut()
        .rev()
        .find(|call| call["tag"] == kind.as_str() && call["title"] == summary.as_str() && call.get("steps").is_none());
    let Some(script) = script else { return false };
    script["steps"] = json!(steps);
    true
}

fn is_question(entry: &Entry) -> bool {
    matches!(entry, Entry::Tool { name, .. } if name == ToolName::AskUserQuestion.as_str())
}

fn picked(output: &str, question: &str) -> String {
    let key = format!("\"{question}\" = \"");
    output
        .split_once(&key)
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(answer, _)| answer)
        .filter(|answer| *answer != "(no answer)")
        .unwrap_or_default()
        .to_owned()
}

fn answered(entry: &Entry) -> Option<Value> {
    let Entry::Tool { state, output, input, .. } = entry else { return None };
    if *state == ToolState::Running {
        return None;
    }
    let output = output.as_deref().unwrap_or_default();
    let questions: Vec<Value> = input.as_ref()?["questions"]
        .as_array()?
        .iter()
        .map(|question| {
            json!({
                "question": question["question"].as_str().unwrap_or_default(),
                "header": question["header"].as_str().unwrap_or_default(),
                "multiSelect": question["multiSelect"].as_bool().unwrap_or(false),
                "options": question["options"].as_array().cloned().unwrap_or_default(),
            })
        })
        .collect();
    let answers: Vec<String> =
        questions.iter().map(|question| picked(output, question["question"].as_str().unwrap_or_default())).collect();
    Some(json!({ "type": "ask", "questions": questions, "answers": answers }))
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

pub fn events(shown: &Shown, pending: &[AgentAsk]) -> Vec<Value> {
    let entries = &shown.transcript.entries;
    let time = |at: usize| shown.times.get(at).copied().unwrap_or(0);
    let mut listed: Vec<Value> = Vec::new();
    let mut tools: Option<(usize, Vec<Value>)> = None;
    let plan_ask = pending.iter().find(|ask| matches!(ask.kind, AskKind::Plan { .. }));
    let mut plan_taken = false;
    let close = |tools: &mut Option<(usize, Vec<Value>)>, listed: &mut Vec<Value>| {
        if let Some((first, calls)) = tools.take() {
            let live = calls.iter().any(|call| call["state"] == "running");
            let event = json!({ "type": "tools", "calls": calls, "seconds": 0, "live": live });
            listed.push(stamped(event, format!("e{first}"), time(first)));
        }
    };
    for (at, entry) in entries.iter().enumerate() {
        if is_question(entry) {
            close(&mut tools, &mut listed);
            if let Some(event) = answered(entry) {
                listed.push(stamped(event, format!("e{at}"), time(at)));
            }
            continue;
        }
        if let Some(found) = call(entry, shown.took(at)) {
            let (_, calls) = tools.get_or_insert_with(|| (at, Vec::new()));
            if !into_script(calls, entry) {
                calls.push(found);
            }
            continue;
        }
        close(&mut tools, &mut listed);
        let attached = match at.checked_sub(1).map(|before| &entries[before]) {
            Some(Entry::Attached { names }) => names.len(),
            _ => 0,
        };
        let Some(event) = single(entry, attached) else { continue };
        let waiting_plan = matches!(entry, Entry::Plan { outcome: Outcome::Waiting, .. });
        let id = match plan_ask {
            Some(ask) if waiting_plan && !plan_taken => {
                plan_taken = true;
                ask.id.0.clone()
            }
            _ => format!("e{at}"),
        };
        listed.push(stamped(event, id, time(at)));
    }
    close(&mut tools, &mut listed);
    let last = shown.times.last().copied().unwrap_or(0);
    for ask in pending.iter().filter(|ask| !(plan_taken && plan_ask.is_some_and(|plan| plan.id == ask.id))) {
        listed.push(ask_event(ask, last));
    }
    listed
}

fn asked(event: &Value) -> Option<Vec<&str>> {
    if event["type"] != "ask" {
        return None;
    }
    Some(event["questions"].as_array()?.iter().map(|question| question["question"].as_str().unwrap_or_default()).collect())
}

fn from_transcript(event: &Value) -> bool {
    event["id"].as_str().is_some_and(|id| id.starts_with('e') && id[1..].chars().all(|c| c.is_ascii_digit()))
}

fn same_ask(known: &Value, event: &Value) -> bool {
    from_transcript(known) && asked(event).is_some() && asked(known) == asked(event)
}

pub fn merge_answered(events: &mut Vec<Value>, answered: Vec<Value>) {
    for event in answered {
        let at = events.iter().position(|known| known["id"] == event["id"]);
        let at = at.or_else(|| events.iter().position(|known| same_ask(known, &event)));
        match at {
            Some(at) => events[at] = event,
            None => events.push(event),
        }
    }
    events.sort_by(|left, right| left["at"].as_str().cmp(&right["at"].as_str()));
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

fn chose(said: &str, label: &str, multi: bool) -> bool {
    if said == label {
        return true;
    }
    multi
        && (said.starts_with(&format!("{label}, "))
            || said.ends_with(&format!(", {label}"))
            || said.contains(&format!(", {label}, ")))
}

fn answers(questions: &[domain::Question], given: &Value) -> Vec<Vec<usize>> {
    let given: Vec<&str> =
        given.as_array().map(|all| all.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    questions
        .iter()
        .enumerate()
        .map(|(at, question)| {
            let said = given.get(at).copied().unwrap_or_default();
            question
                .options
                .iter()
                .enumerate()
                .filter(|(_, choice)| chose(said, &choice.label, question.multi))
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

pub fn handed_off(decision: Decision) -> Decision {
    match decision {
        Decision::Approve(_) => Decision::Approve(PermissionMode::Plan),
        other => other,
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
