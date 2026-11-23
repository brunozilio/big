use async_trait::async_trait;
use domain::connection::Capability;
use jiff::{SignedDuration, Timestamp};
use serde_json::{Value, json};

use super::adf::adf_to_markdown;
use super::{Api, jql_key};
use crate::contracts;
use crate::error::ActionError;
use crate::http::{Http, Method};
use crate::provider::{Credentials, EventBatch, EventSource, ExternalEvent, text_of, timestamp};

const PAGES: usize = 10;
const MARGIN: SignedDuration = SignedDuration::from_hours(15);

#[derive(Debug, Clone, Copy, Default)]
pub struct JiraEvents;

pub fn events_jql(project: &str, since: Timestamp) -> Result<String, ActionError> {
    let from = since.checked_sub(MARGIN).unwrap_or(since);
    let day = from.strftime("%Y-%m-%d %H:%M");
    Ok(format!("project = \"{}\" AND updated >= \"{day}\" ORDER BY updated ASC", jql_key(project)?))
}

struct Collector<'a> {
    creds: &'a Credentials,
    since: Timestamp,
    events: Vec<ExternalEvent>,
}

impl Collector<'_> {
    fn push(&mut self, id: String, kind: &str, subject: &str, data: Value, at: &str) {
        let Some(at) = timestamp(at).filter(|at| *at > self.since) else { return };
        self.events.push(ExternalEvent {
            id,
            connection: self.creds.connection.clone(),
            capability: Capability::from(contracts::TRACKER),
            kind: kind.to_owned(),
            subject: subject.to_owned(),
            data,
            at,
        });
    }

    fn issue(&mut self, issue: &Value) {
        let key = text_of(issue, "/key");
        let fields = &issue["fields"];
        let summary = json!({ "title": text_of(fields, "/summary"), "status": text_of(fields, "/status/name") });
        self.push(format!("jira:{key}:created"), "tracker.issue.created", &key, summary, &text_of(fields, "/created"));
        for history in issue["changelog"]["histories"].as_array().map(Vec::as_slice).unwrap_or_default() {
            self.history(&key, history);
        }
        for comment in fields["comment"]["comments"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let data = json!({
                "id": text_of(comment, "/id"),
                "author": text_of(comment, "/author/displayName"),
                "body": adf_to_markdown(&comment["body"]),
            });
            let id = format!("jira:{key}:comment:{}", text_of(comment, "/id"));
            self.push(id, "tracker.comment.added", &key, data, &text_of(comment, "/created"));
        }
    }

    fn history(&mut self, key: &str, history: &Value) {
        let at = text_of(history, "/created");
        let author = text_of(history, "/author/displayName");
        for item in history["items"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let field = text_of(item, "/field");
            let kind = match field.to_ascii_lowercase().as_str() {
                "status" => "tracker.status.changed",
                "flagged" => "tracker.impediment.changed",
                _ => continue,
            };
            let data = json!({
                "from": item["fromString"],
                "to": item["toString"],
                "author": author,
            });
            let id = format!("jira:{key}:{field}:{}", text_of(history, "/id"));
            self.push(id, kind, key, data, &at);
        }
    }
}

fn latest(issues: &[Value], since: Timestamp) -> Timestamp {
    issues.iter().filter_map(|issue| timestamp(&text_of(issue, "/fields/updated"))).fold(since, Timestamp::max)
}

#[async_trait]
impl EventSource for JiraEvents {
    async fn poll(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        cursor: Option<&str>,
    ) -> Result<EventBatch, ActionError> {
        let Some(since) = cursor.and_then(timestamp) else {
            return Ok(EventBatch { events: Vec::new(), cursor: Timestamp::now().to_string() });
        };
        let project =
            creds.setting("project").ok_or_else(|| ActionError::permanent("the connection has no project to watch"))?;
        let api = Api::new(http, creds)?;
        let jql = events_jql(project, since)?;
        let mut collector = Collector { creds, since, events: Vec::new() };
        let mut newest = since;
        let mut token: Option<String> = None;
        for _ in 0..PAGES {
            let body = json!({
                "jql": jql,
                "fields": ["summary", "status", "created", "updated", "comment"],
                "expand": "changelog",
                "maxResults": 100,
                "nextPageToken": token,
            });
            let page = api.send(Method::Post, "/search/jql", &body).await?;
            let issues = page["issues"].as_array().map(Vec::as_slice).unwrap_or_default();
            issues.iter().for_each(|issue| collector.issue(issue));
            newest = latest(issues, newest);
            token = page["nextPageToken"].as_str().map(str::to_owned).filter(|_| page["isLast"] != json!(true));
            if token.is_none() {
                break;
            }
        }
        collector.events.sort_by_key(|event| event.at);
        Ok(EventBatch { events: collector.events, cursor: newest.to_string() })
    }
}
