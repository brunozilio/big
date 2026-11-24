use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use connectors::oauth::Dialect;
use connectors::{
    AccountOption, ActionError, AuthSpec, Credentials, DeviceApp, EventBatch, EventSource, ExternalEvent, Http,
    Provider, ProviderDescriptor,
};
use domain::connection::{Capability, ConnectionId, ProviderKey};
use serde_json::{Value, json};

pub const KEY: &str = "tracker-fake";
pub const CONNECTION: &str = "trk";
pub const STATUSES: [&str; 4] = ["A fazer", "Fazendo", "Revisando", "Feito"];
const STAGES: [&str; 3] = ["Etapa 1", "Etapa 2", "Etapa 3"];

#[derive(Debug, Clone, Default)]
pub struct Issue {
    pub title: String,
    pub status: String,
    pub flagged: bool,
    pub comments: Vec<String>,
    pub key: Option<String>,
}

#[derive(Debug, Default)]
pub struct State {
    pub issues: BTreeMap<String, Issue>,
    pub created: usize,
    pub transitions: Vec<(String, String)>,
    pub comments_posted: usize,
    pub events: Vec<ExternalEvent>,
    pub polls: usize,
    keyed_comments: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Tracker {
    state: Mutex<State>,
}

impl Tracker {
    pub fn leaked() -> &'static Tracker {
        Box::leak(Box::new(Tracker::default()))
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn add_issue(&self, key: &str, title: &str, status: &str) {
        let issue = Issue { title: title.into(), status: status.into(), ..Issue::default() };
        self.state().issues.insert(key.into(), issue);
    }

    pub fn statuses_of(&self, key: &str) -> Vec<String> {
        self.state().transitions.iter().filter(|(issue, _)| issue == key).map(|(_, to)| to.clone()).collect()
    }

    pub fn move_outside(&self, key: &str, to: &str) {
        let mut state = self.state();
        let Some(issue) = state.issues.get_mut(key) else { return };
        let from = std::mem::replace(&mut issue.status, to.into());
        let count = state.events.len();
        state.events.push(ExternalEvent {
            id: format!("fake:{key}:status:{count}"),
            connection: ConnectionId::from(CONNECTION),
            capability: Capability::from("tracker"),
            kind: "tracker.status.changed".into(),
            subject: key.into(),
            data: json!({ "from": from, "to": to, "author": "Ana" }),
            at: jiff::Timestamp::now(),
        });
    }
}

fn text<'a>(args: &'a Value, name: &str) -> Result<&'a str, ActionError> {
    args[name].as_str().filter(|text| !text.is_empty()).ok_or_else(|| ActionError::missing(name))
}

fn summary(key: &str, issue: &Issue) -> Value {
    json!({ "ref": key, "title": issue.title, "status": issue.status, "url": format!("https://tracker.test/{key}") })
}

fn create(state: &mut State, args: &Value, key: &str) -> Result<Value, ActionError> {
    let title = text(args, "title")?;
    if let Some((found, issue)) = state.issues.iter().find(|(_, issue)| issue.key.as_deref() == Some(key)) {
        let mut answer = summary(found, issue);
        answer["created"] = json!(false);
        return Ok(answer);
    }
    state.created += 1;
    let reference = format!("K-{}", 100 + state.created);
    let issue = Issue { title: title.into(), status: STATUSES[0].into(), key: Some(key.into()), ..Issue::default() };
    let mut answer = summary(&reference, &issue);
    answer["created"] = json!(true);
    state.issues.insert(reference, issue);
    Ok(answer)
}

fn transition(state: &mut State, args: &Value) -> Result<Value, ActionError> {
    let key = text(args, "issue")?.to_owned();
    let to = text(args, "status")?.to_owned();
    if !STATUSES.contains(&to.as_str()) && !STAGES.contains(&to.as_str()) {
        return Err(ActionError::permanent(format!("{key} cannot go to {to}")));
    }
    let issue = state.issues.get_mut(&key).ok_or_else(|| ActionError::permanent(format!("no issue {key}")))?;
    let changed = issue.status != to;
    issue.status.clone_from(&to);
    if changed {
        state.transitions.push((key.clone(), to.clone()));
    }
    Ok(json!({ "ref": key, "status": to, "changed": changed }))
}

fn comment(state: &mut State, args: &Value, key: &str) -> Result<Value, ActionError> {
    let issue = text(args, "issue")?.to_owned();
    let body = text(args, "body")?.to_owned();
    if !key.is_empty() && state.keyed_comments.iter().any(|known| known == key) {
        return Ok(json!({ "id": key, "author": null, "body": body, "at": "", "created": false }));
    }
    state.keyed_comments.push(key.to_owned());
    state.comments_posted += 1;
    state.issues.entry(issue).or_default().comments.push(body.clone());
    Ok(json!({ "id": key, "author": null, "body": body, "at": "", "created": true }))
}

fn answer(state: &mut State, action: &str, args: &Value, key: &str) -> Result<Value, ActionError> {
    match action {
        "tracker.create_issue" => create(state, args, key),
        "tracker.issue" => {
            let reference = text(args, "issue")?;
            let issue = state.issues.get(reference).ok_or_else(|| ActionError::permanent("no issue"))?;
            let mut found = summary(reference, issue);
            found["description"] = json!("");
            found["impediment"] = json!({ "on": issue.flagged, "reason": null });
            found["subtasks"] = json!([]);
            found["attachments"] = json!([]);
            found["comments"] = json!(
                issue
                    .comments
                    .iter()
                    .map(|body| json!({ "id": "c", "author": null, "body": body, "at": "" }))
                    .collect::<Vec<_>>()
            );
            Ok(found)
        }
        "tracker.transition" => transition(state, args),
        "tracker.comment" => comment(state, args, key),
        "tracker.set_impediment" => {
            let on = args["on"].as_bool().unwrap_or(false);
            if let Some(issue) = state.issues.get_mut(text(args, "issue")?) {
                issue.flagged = on;
            }
            Ok(json!({ "on": on, "reason": args["reason"] }))
        }
        "tracker.statuses" => Ok(json!(
            STATUSES
                .iter()
                .chain(STAGES.iter())
                .map(|name| json!({ "id": name, "name": name, "category": null }))
                .collect::<Vec<_>>()
        )),
        "tracker.projects" => Ok(json!([{ "id": "1", "key": "K", "name": "Kanban" }])),
        "tracker.search" => {
            let issues: Vec<Value> = state.issues.iter().map(|(key, issue)| summary(key, issue)).collect();
            Ok(json!({ "issues": issues, "cursor": null }))
        }
        "tests.cases" => Ok(json!([])),
        "tests.create_case" => Ok(json!({ "ref": "K-T1", "title": args["title"], "url": "" })),
        "tests.create_cycle" => Ok(json!({ "ref": "K-R1", "url": "" })),
        "tests.record" => Ok(json!({ "ref": "K-E1" })),
        other => Err(ActionError::permanent(format!("the fake tracker does not know {other}"))),
    }
}

#[async_trait]
impl Provider for Tracker {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from(KEY),
            name: "Tracker".into(),
            logo: "<svg/>".into(),
            capabilities: vec![
                Capability::from(connectors::contracts::TRACKER),
                Capability::from(connectors::contracts::TESTS),
            ],
            auth: AuthSpec::Device(DeviceApp {
                client_id: "c".into(),
                client_secret: None,
                device_endpoint: "https://login.test/device".into(),
                token_endpoint: "https://login.test/token".into(),
                scopes: Vec::new(),
                start_fields: Vec::new(),
                dialect: Dialect::Form,
            }),
            actions: [connectors::contracts::tracker(), connectors::contracts::tests()].concat(),
        }
    }

    async fn call(
        &self,
        _: &dyn Http,
        _: &Credentials,
        action: &str,
        args: &Value,
        key: &str,
    ) -> Result<Value, ActionError> {
        answer(&mut self.state(), action, args, key)
    }

    async fn accounts(&self, _: &dyn Http, _: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        Ok(Vec::new())
    }

    fn events(&self) -> Option<&dyn EventSource> {
        Some(self)
    }
}

#[async_trait]
impl EventSource for Tracker {
    async fn poll(&self, _: &dyn Http, _: &Credentials, cursor: Option<&str>) -> Result<EventBatch, ActionError> {
        let mut state = self.state();
        state.polls += 1;
        let seen: usize = cursor.and_then(|text| text.parse().ok()).unwrap_or(0);
        let fresh: Vec<ExternalEvent> = state.events.iter().skip(seen).cloned().collect();
        let total = state.events.len();
        Ok(EventBatch { events: if cursor.is_some() { fresh } else { Vec::new() }, cursor: total.to_string() })
    }
}
