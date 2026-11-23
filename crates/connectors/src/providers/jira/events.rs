use async_trait::async_trait;
use domain::connection::Capability;
use jiff::{SignedDuration, Timestamp};
use serde_json::{Value, json};

use super::tracker::comment_of;
use super::{Api, jql_key, listed};
use crate::contracts;
use crate::error::ActionError;
use crate::http::Http;
use crate::provider::{Credentials, EventBatch, EventSource, ExternalEvent, text_of, timestamp};

const PAGES: usize = 10;
const DETAIL: [&str; 5] = ["summary", "status", "created", "updated", "comment"];
const MARGIN: SignedDuration = SignedDuration::from_hours(15);

#[derive(Debug, Clone, Copy, Default)]
pub struct JiraEvents;

pub fn events_jql(project: &str, since: Timestamp) -> Result<String, ActionError> {
    let from = since.checked_sub(MARGIN).unwrap_or(since);
    let day = from.strftime("%Y-%m-%d %H:%M");
    Ok(format!("project = \"{}\" AND updated >= \"{day}\" ORDER BY updated ASC", jql_key(project)?))
}

pub(super) struct Collector<'a> {
    pub(super) creds: &'a Credentials,
    pub(super) since: Timestamp,
    pub(super) events: Vec<ExternalEvent>,
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
        let summary =
            json!({ "ref": key, "title": text_of(fields, "/summary"), "status": text_of(fields, "/status/name") });
        self.push(format!("jira:{key}:created"), "tracker.issue.created", &key, summary, &text_of(fields, "/created"));
        for history in issue["changelog"]["histories"].as_array().map(Vec::as_slice).unwrap_or_default() {
            self.history(&key, history);
        }
        for comment in fields["comment"]["comments"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let data = comment_of(comment);
            let id = format!("jira:{key}:comment:{}", text_of(comment, "/id"));
            self.push(id, "tracker.comment.added", &key, data, &text_of(comment, "/created"));
        }
    }

    pub(super) fn history(&mut self, key: &str, history: &Value) {
        let at = text_of(history, "/created");
        let author = text_of(history, "/author/displayName");
        for item in history["items"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let field = text_of(item, "/field");
            let flag = |name: &str| !text_of(item, name).trim().is_empty();
            let (kind, data) = match field.to_ascii_lowercase().as_str() {
                "status" => (
                    "tracker.status.changed",
                    json!({ "from": item["fromString"], "to": item["toString"], "author": author }),
                ),
                "flagged" => (
                    "tracker.impediment.changed",
                    json!({ "from": flag("/fromString"), "to": flag("/toString"), "author": author }),
                ),
                _ => continue,
            };
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
        let api = Api::new(http, creds).await?;
        let jql = events_jql(project, since)?;
        let mut collector = Collector { creds, since, events: Vec::new() };
        let mut newest = since;
        let mut token: Option<String> = None;
        for _ in 0..PAGES {
            let page = api.search(&jql, &["updated"], token.as_deref()).await?;
            let issues = listed(&page, "issues");
            for found in issues {
                let arguments = json!({
                    "issueIdOrKey": text_of(found, "/key"), "fields": DETAIL, "expand": "changelog",
                    "responseContentFormat": "markdown",
                });
                collector.issue(&api.tool("getJiraIssue", arguments).await?);
            }
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
