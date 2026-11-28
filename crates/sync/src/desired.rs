use connectors::ExternalEvent;
use connectors::contracts::{code, pipelines, tracker};
use domain::connection::Connection;
use domain::effect::{Call, Effect, EffectOutcome};
use domain::workflow::{EffectEntry, Impediment, ItemId, WorkItem};
use serde_json::Value;

use crate::policy::ReconcilePolicy;

pub mod kinds {
    pub const STATUS_CHANGED: &str = "tracker.status.changed";
    pub const IMPEDIMENT_CHANGED: &str = "tracker.impediment.changed";
    pub const PR_MERGED: &str = "code.pr.merged";
    pub const PR_CLOSED: &str = "code.pr.closed";
    pub const PR_UPDATED: &str = "code.pr.updated";
    pub const RUN_FINISHED: &str = "pipelines.run.finished";
    pub const DEPLOYMENT_WAITING: &str = "pipelines.deployment.waiting";
    pub const ALL: [&str; 7] =
        [STATUS_CHANGED, IMPEDIMENT_CHANGED, PR_MERGED, PR_CLOSED, PR_UPDATED, RUN_FINISHED, DEPLOYMENT_WAITING];
}

pub const MERGED: &str = "merged";
pub const CLOSED: &str = "closed";
pub const OPEN: &str = "open";
pub const WAITING: &str = "waiting";
pub const FINISHED: [&str; 4] = ["passed", "failed", "cancelled", "skipped"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueRef {
    pub issue: String,
    pub connection: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRef {
    pub reference: String,
    pub state: Option<String>,
    pub connection: Option<String>,
}

impl PullRef {
    pub fn subject(&self) -> String {
        self.reference.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRef {
    pub reference: String,
    pub state: Option<String>,
    pub connection: Option<String>,
}

impl RunRef {
    pub fn subject(&self) -> String {
        self.reference.clone()
    }

    pub fn finished(&self) -> bool {
        self.state.as_deref().is_some_and(|state| FINISHED.contains(&state))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Wait {
    pub kind: String,
    pub filter: Value,
}

impl Wait {
    pub fn capability(&self) -> &str {
        self.kind.split_once('.').map_or(self.kind.as_str(), |(capability, _)| capability)
    }

    pub fn connection(&self) -> Option<&str> {
        self.filter.get("connection").and_then(Value::as_str)
    }

    pub fn subject(&self) -> Option<&str> {
        self.filter.get("subject").and_then(Value::as_str)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Desired {
    pub item: ItemId,
    pub workflow: String,
    pub status: Option<String>,
    pub impediment: Option<Impediment>,
    pub issue: Option<IssueRef>,
    pub pulls: Vec<PullRef>,
    pub runs: Vec<RunRef>,
    pub waits: Vec<Wait>,
    pub policy: ReconcilePolicy,
    pub owner: String,
}

impl Desired {
    pub fn subjects(&self) -> Vec<String> {
        let issue = self.issue.iter().map(|issue| issue.issue.clone());
        let pulls = self.pulls.iter().map(PullRef::subject);
        issue.chain(pulls).chain(self.runs.iter().map(RunRef::subject)).collect()
    }

    pub fn concerns(&self, event: &ExternalEvent) -> bool {
        self.subjects().contains(&event.subject)
    }

    pub fn waits_on(&self, connection: &Connection) -> bool {
        self.waits.iter().any(|wait| {
            let space = wait.capability();
            (connection.offers(&space.into()) || connection.provider.as_str() == space)
                && wait.connection().is_none_or(|id| id == connection.id.as_str())
        })
    }

    pub fn waits_for(&self, capability: &str) -> bool {
        self.waits.iter().any(|wait| wait.capability() == capability)
    }

    pub fn uses(&self, connection: &str) -> bool {
        let issue = self.issue.iter().filter_map(|issue| issue.connection.as_deref());
        let pulls = self.pulls.iter().filter_map(|pull| pull.connection.as_deref());
        let runs = self.runs.iter().filter_map(|run| run.connection.as_deref());
        issue.chain(pulls).chain(runs).any(|id| id == connection)
    }

    fn learn_call(&mut self, call: &Call, value: &Value) {
        let args = &call.args;
        let connection = call.connection.clone();
        match call.action.as_str() {
            tracker::CREATE_ISSUE => self.learn_issue(text(value, "ref"), connection),
            action if action.starts_with("tracker.") => self.learn_issue(text(args, "issue"), connection),
            code::OPEN_PULL_REQUEST | code::PULL_REQUEST | code::MERGE => {
                let merged = call.action == code::MERGE && value.get("merged") == Some(&Value::Bool(true));
                let state = if merged { Some(MERGED.to_owned()) } else { text(value, "state") };
                if let Some(reference) = text(value, "ref").or_else(|| text(args, "pr")) {
                    self.learn_pull(PullRef { reference, state, connection });
                }
            }
            pipelines::RUN => self.learn_waited_run(call, text(value, "state")),
            _ => {}
        }
    }

    fn learn_waited_run(&mut self, call: &Call, state: Option<String>) {
        if call.action != pipelines::RUN {
            return;
        }
        if let Some(reference) = text(&call.args, "run") {
            self.learn_run(RunRef { reference, state, connection: call.connection.clone() });
        }
    }

    fn learn_sources(&mut self, sources: &Value) {
        for source in sources.as_array().map(Vec::as_slice).unwrap_or_default() {
            let (subject, connection) = (text(source, "subject"), text(source, "connection"));
            match source["kind"].as_str() {
                Some("issue") => self.learn_issue(subject, connection),
                Some("pull_request") => {
                    if let Some(reference) = subject {
                        self.learn_pull(PullRef { reference, state: None, connection });
                    }
                }
                _ => {}
            }
        }
    }

    fn learn_output(&mut self, output: &Value) {
        if self.issue.is_none() {
            self.learn_issue(text(output, "issue"), None);
        }
        if let Some(reference) = text(output, "pr") {
            let state = if output["merged"] == Value::Bool(true) { Some(MERGED.to_owned()) } else { None };
            self.learn_pull(PullRef { reference, state, connection: text(output, "connection") });
        }
        if let Some(reference) = text(output, "run") {
            self.learn_run(RunRef { reference, state: text(output, "state"), connection: text(output, "connection") });
        }
    }

    fn learn_issue(&mut self, issue: Option<String>, connection: Option<String>) {
        if let Some(issue) = issue {
            match &mut self.issue {
                Some(known) if known.issue == issue => {
                    known.connection = known.connection.take().or(connection);
                }
                Some(_) => {}
                None => self.issue = Some(IssueRef { issue, connection }),
            }
        }
    }

    fn learn_pull(&mut self, pull: PullRef) {
        match self.pulls.iter_mut().find(|known| known.reference == pull.reference) {
            Some(known) => {
                known.state = pull.state.or(known.state.take());
                known.connection = known.connection.take().or(pull.connection);
            }
            None => self.pulls.push(pull),
        }
    }

    fn learn_run(&mut self, run: RunRef) {
        match self.runs.iter_mut().find(|known| known.reference == run.reference) {
            Some(known) => {
                known.state = run.state.or(known.state.take());
                known.connection = known.connection.take().or(run.connection);
            }
            None => self.runs.push(run),
        }
    }

    pub fn observe(&mut self, names: &[&str], subject: &str, data: &Value) {
        let kind = names.iter().copied().find(|name| kinds::ALL.contains(name)).unwrap_or_default();
        let pull_state = match kind {
            kinds::PR_MERGED => Some(MERGED.to_owned()),
            kinds::PR_CLOSED => Some(CLOSED.to_owned()),
            kinds::PR_UPDATED => text(data, "state"),
            _ => None,
        };
        if let Some(state) = pull_state
            && let Some(pull) = self.pulls.iter_mut().find(|pull| pull.subject() == subject)
        {
            pull.state = Some(state);
        }
        let run_state = match kind {
            kinds::RUN_FINISHED => text(data, "state").or_else(|| Some(FINISHED[0].to_owned())),
            kinds::DEPLOYMENT_WAITING => Some(WAITING.to_owned()),
            _ => None,
        };
        if let Some(state) = run_state
            && let Some(run) = self.runs.iter_mut().find(|run| run.subject() == subject)
        {
            run.state = Some(state);
        }
    }
}

fn text(value: &Value, name: &str) -> Option<String> {
    value.get(name).and_then(Value::as_str).filter(|text| !text.is_empty()).map(str::to_owned)
}

fn ordered(item: &WorkItem) -> Vec<&EffectEntry> {
    let mut entries: Vec<&EffectEntry> = item.effects.values().collect();
    entries.sort_by_key(|entry| entry.request.key.seq);
    entries
}

fn event_of(outcome: Option<&EffectOutcome>) -> Option<(Vec<String>, String, Value)> {
    let Some(EffectOutcome::Ok { value }) = outcome else { return None };
    let aliases = value["aliases"].as_array().map(Vec::as_slice).unwrap_or_default();
    let mut names = vec![text(value, "kind")?];
    names.extend(aliases.iter().filter_map(Value::as_str).map(str::to_owned));
    Some((names, text(value, "subject")?, value.get("data").cloned().unwrap_or(Value::Null)))
}

pub fn desired_of(item: &WorkItem, policy: ReconcilePolicy) -> Desired {
    let mut desired = Desired {
        item: item.id.clone(),
        workflow: item.workflow.clone(),
        status: item.status.clone(),
        impediment: item.impediment.clone(),
        issue: text(&item.input, "issue").map(|issue| IssueRef { issue, connection: None }),
        pulls: Vec::new(),
        runs: Vec::new(),
        waits: Vec::new(),
        policy,
        owner: domain::principal::owner_in(&item.input).to_owned(),
    };
    desired.learn_sources(&item.input["sources"]);
    let entries = ordered(item);
    for entry in &entries {
        match (&entry.request.effect, &entry.outcome) {
            (Effect::ConnectorCall(call), Some(EffectOutcome::Ok { value })) => desired.learn_call(call, value),
            (Effect::WaitPoll { call, .. }, None) => desired.learn_waited_run(call, None),
            (Effect::WaitEvent { kind, filter, .. }, None) => {
                desired.waits.push(Wait { kind: kind.clone(), filter: filter.clone() });
            }
            _ => {}
        }
    }
    item.steps.iter().for_each(|step| desired.learn_output(&step.output));
    let waited = entries
        .iter()
        .filter(|entry| matches!(entry.request.effect, Effect::WaitEvent { .. }))
        .filter_map(|entry| event_of(entry.outcome.as_ref()));
    for (names, subject, data) in waited {
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        desired.observe(&names, &subject, &data);
    }
    let mut seen: Vec<_> = item.observed.values().collect();
    seen.sort_by_key(|seen| seen.at);
    seen.iter().for_each(|seen| desired.observe(&seen.names(), &seen.subject, &seen.data));
    desired
}

#[cfg(test)]
mod tests;
