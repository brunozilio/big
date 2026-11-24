use std::collections::BTreeMap;

use async_trait::async_trait;
use domain::connection::Capability;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::Api;
use super::refs::{pull_ref, run_ref};
use super::states::{state, verdict};
use crate::{contracts, idempotency};
use crate::error::ActionError;
use crate::http::{Http, Method};
use crate::provider::{Credentials, EventBatch, EventSource, ExternalEvent, text_of, timestamp};

#[derive(Debug, Clone, Copy, Default)]
pub struct GithubEvents;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub since: Timestamp,
    #[serde(default)]
    pub etags: BTreeMap<String, String>,
}

struct Poll<'a> {
    api: Api<'a>,
    creds: &'a Credentials,
    since: Timestamp,
    newest: Timestamp,
    etags: BTreeMap<String, String>,
    events: Vec<ExternalEvent>,
}

impl Poll<'_> {
    async fn changed(&mut self, path: &str) -> Result<Option<Vec<Value>>, ActionError> {
        let mut request = self.api.request(Method::Get, path);
        if let Some(etag) = self.etags.get(path) {
            request = request.header("if-none-match", etag.clone());
        }
        let response = self.api.raw(request).await?;
        if response.status == 304 {
            return Ok(None);
        }
        if let Some(etag) = response.header("etag") {
            self.etags.insert(path.to_owned(), etag.to_owned());
        }
        let body = response.json::<Value>()?;
        let listed = body.get("workflow_runs").unwrap_or(&body);
        Ok(Some(listed.as_array().cloned().unwrap_or_default()))
    }

    fn push(&mut self, capability: &str, kind: &str, id: String, subject: String, data: Value, at: &str) {
        let Some(at) = timestamp(at).filter(|at| *at > self.since) else { return };
        self.newest = self.newest.max(at);
        self.events.push(ExternalEvent {
            id,
            connection: self.creds.connection.clone(),
            capability: Capability::from(capability),
            kind: kind.to_owned(),
            subject,
            data,
            at,
        });
    }

    async fn pulls(&mut self, repo: &str) -> Result<(), ActionError> {
        let path = format!("/repos/{repo}/pulls?state=all&sort=updated&direction=desc&per_page=50");
        let Some(pulls) = self.changed(&path).await? else { return Ok(()) };
        let since = self.since;
        let fresh = pulls.iter().filter(|pull| timestamp(&text_of(pull, "/updated_at")).is_some_and(|at| at > since));
        for pull in fresh {
            let number = pull["number"].as_u64().unwrap_or_default();
            let subject = pull_ref(repo, number);
            let updated = text_of(pull, "/updated_at");
            let (kind, at, state) = match (pull["merged_at"].as_str(), pull["state"].as_str()) {
                (Some(merged), _) => ("code.pr.merged", merged.to_owned(), "merged"),
                (None, Some("closed")) => ("code.pr.closed", text_of(pull, "/closed_at"), "closed"),
                _ => ("code.pr.updated", updated.clone(), "open"),
            };
            let head = text_of(pull, "/head/ref");
            let base = text_of(pull, "/base/ref");
            let data = json!({ "pr": subject, "repo": repo, "number": number, "head": head, "base": base, "state": state, "title": pull["title"] });
            let id = format!("github:{repo}:pr:{number}:{kind}:{at}");
            self.push(contracts::CODE, kind, id, subject.clone(), data, &at);
            self.reviews(repo, pull, &subject).await?;
        }
        Ok(())
    }

    async fn reviews(&mut self, repo: &str, pull: &Value, subject: &str) -> Result<(), ActionError> {
        let number = pull["number"].as_u64().unwrap_or_default();
        let reviews = self.api.get(&format!("/repos/{repo}/pulls/{number}/reviews?per_page=100")).await?;
        for review in reviews.as_array().map(Vec::as_slice).unwrap_or_default() {
            let data = json!({
                "pr": subject,
                "repo": repo,
                "number": number,
                "head": text_of(pull, "/head/ref"),
                "base": text_of(pull, "/base/ref"),
                "title": pull["title"],
                "verdict": verdict(&review["state"]),
                "author": text_of(review, "/user/login"),
                "body": idempotency::strip(review["body"].as_str().unwrap_or_default()),
            });
            let id = format!("github:{repo}:review:{}", review["id"]);
            self.push(
                contracts::CODE,
                "code.review.submitted",
                id,
                subject.to_owned(),
                data,
                &text_of(review, "/submitted_at"),
            );
        }
        Ok(())
    }

    async fn runs(&mut self, repo: &str) -> Result<(), ActionError> {
        let path = format!("/repos/{repo}/actions/runs?per_page=50");
        let Some(runs) = self.changed(&path).await? else { return Ok(()) };
        for run in &runs {
            let kind = match run["status"].as_str() {
                Some("completed") => "pipelines.run.finished",
                Some("waiting") => "pipelines.deployment.waiting",
                _ => continue,
            };
            let id = format!("github:{repo}:run:{}:{}:{kind}", run["id"], run["run_attempt"]);
            let subject = run_ref(repo, run["id"].as_u64().unwrap_or_default());
            let data = json!({
                "run": subject,
                "repo": repo,
                "pipeline": run["name"],
                "head": run["head_branch"],
                "sha": run["head_sha"],
                "state": state(&run["status"], &run["conclusion"]),
            });
            self.push(contracts::PIPELINES, kind, id, subject, data, &text_of(run, "/updated_at"));
        }
        Ok(())
    }
}

fn repos(creds: &Credentials) -> Vec<String> {
    let mut repos = creds.settings_list("repos");
    if let Some(single) = creds.setting("repo") {
        repos.push(single.to_owned());
    }
    repos
}

#[async_trait]
impl EventSource for GithubEvents {
    async fn poll(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        cursor: Option<&str>,
    ) -> Result<EventBatch, ActionError> {
        let Some(cursor) = cursor.and_then(|text| serde_json::from_str::<Cursor>(text).ok()) else {
            let fresh = Cursor { since: Timestamp::now(), etags: BTreeMap::new() };
            return Ok(EventBatch {
                events: Vec::new(),
                cursor: serde_json::to_string(&fresh).map_err(ActionError::decode)?,
            });
        };
        let mut poll = Poll {
            api: Api::new(http, creds),
            creds,
            since: cursor.since,
            newest: cursor.since,
            etags: cursor.etags,
            events: Vec::new(),
        };
        for repo in repos(creds) {
            poll.pulls(&repo).await?;
            poll.runs(&repo).await?;
        }
        poll.events.sort_by_key(|event| event.at);
        let next = Cursor { since: poll.newest, etags: poll.etags };
        Ok(EventBatch { events: poll.events, cursor: serde_json::to_string(&next).map_err(ActionError::decode)? })
    }
}
