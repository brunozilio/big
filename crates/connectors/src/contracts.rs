use serde_json::{Value, json};

use crate::descriptor::{ActionSpec, Idempotency, object};

pub const TRACKER: &str = "tracker";
pub const CODE: &str = "code";
pub const PIPELINES: &str = "pipelines";
pub const CLOUD: &str = "cloud";

pub mod tracker {
    pub const CREATE_ISSUE: &str = "tracker.create_issue";
    pub const ISSUE: &str = "tracker.issue";
    pub const COMMENT: &str = "tracker.comment";
    pub const STATUSES: &str = "tracker.statuses";
    pub const TRANSITION: &str = "tracker.transition";
    pub const SET_IMPEDIMENT: &str = "tracker.set_impediment";
    pub const SEARCH: &str = "tracker.search";
}

pub mod code {
    pub const OPEN_PULL_REQUEST: &str = "code.open_pull_request";
    pub const PULL_REQUEST: &str = "code.pull_request";
    pub const FILES_DIFF: &str = "code.files_diff";
    pub const REVIEW_COMMENTS: &str = "code.review_comments";
    pub const SUBMIT_REVIEW: &str = "code.submit_review";
    pub const RESOLVE_THREAD: &str = "code.resolve_thread";
    pub const MERGE: &str = "code.merge";
}

pub mod pipelines {
    pub const RUNS: &str = "pipelines.runs";
    pub const RUN: &str = "pipelines.run";
    pub const JOB_LOG: &str = "pipelines.job_log";
    pub const PENDING_DEPLOYMENTS: &str = "pipelines.pending_deployments";
    pub const REVIEW_DEPLOYMENT: &str = "pipelines.review_deployment";
}

pub mod cloud {
    pub const IDENTITY: &str = "cloud.identity";
    pub const ACCOUNTS_AND_ROLES: &str = "cloud.accounts_and_roles";
}

fn list(name: &str) -> Value {
    object(&[(name, "array")], &[])
}

fn issue_output() -> Value {
    object(
        &[("key", "string"), ("title", "string"), ("status", "string"), ("impediment", "boolean")],
        &[("description", "string"), ("subtasks", "array"), ("attachments", "array"), ("comments", "array")],
    )
}

pub fn tracker() -> Vec<ActionSpec> {
    let created = object(&[("key", "string"), ("id", "string"), ("created", "boolean")], &[("url", "string")]);
    let new_issue = object(
        &[("project", "string"), ("title", "string")],
        &[("description", "string"), ("issue_type", "string"), ("parent", "string"), ("labels", "array")],
    );
    vec![
        ActionSpec::write(tracker::CREATE_ISSUE, new_issue, created, Idempotency::Lookup),
        ActionSpec::read(tracker::ISSUE, object(&[("issue", "string")], &[]), issue_output()),
        ActionSpec::write(
            tracker::COMMENT,
            object(&[("issue", "string"), ("body", "string")], &[]),
            object(&[("id", "string"), ("created", "boolean")], &[]),
            Idempotency::Lookup,
        ),
        ActionSpec::read(tracker::STATUSES, object(&[("project", "string")], &[]), list("statuses")),
        ActionSpec::write(
            tracker::TRANSITION,
            object(&[("issue", "string"), ("status", "string")], &[]),
            object(&[("status", "string"), ("changed", "boolean")], &[]),
            Idempotency::Lookup,
        ),
        ActionSpec::write(
            tracker::SET_IMPEDIMENT,
            object(&[("issue", "string"), ("on", "boolean")], &[("reason", "string")]),
            object(&[("impediment", "boolean")], &[]),
            Idempotency::Lookup,
        ),
        ActionSpec::read(
            tracker::SEARCH,
            object(&[], &[("project", "string"), ("text", "string"), ("status", "string"), ("limit", "integer")]),
            list("issues"),
        ),
    ]
}

fn pull_output() -> Value {
    object(
        &[("number", "integer"), ("state", "string"), ("url", "string"), ("head", "string"), ("base", "string")],
        &[("created", "boolean"), ("checks", "object"), ("reviews", "array"), ("merged", "boolean")],
    )
}

fn pull_ref() -> Value {
    object(&[("repo", "string"), ("number", "integer")], &[])
}

pub fn code() -> Vec<ActionSpec> {
    let open = object(
        &[("repo", "string"), ("head", "string"), ("base", "string"), ("title", "string")],
        &[("body", "string"), ("draft", "boolean")],
    );
    let review = object(
        &[("repo", "string"), ("number", "integer"), ("event", "string")],
        &[("body", "string"), ("comments", "array")],
    );
    let merge = object(&[("repo", "string"), ("number", "integer")], &[("method", "string")]);
    vec![
        ActionSpec::write(code::OPEN_PULL_REQUEST, open, pull_output(), Idempotency::Lookup),
        ActionSpec::read(code::PULL_REQUEST, pull_ref(), pull_output()),
        ActionSpec::read(code::FILES_DIFF, pull_ref(), list("files")),
        ActionSpec::read(code::REVIEW_COMMENTS, pull_ref(), list("threads")),
        ActionSpec::write(code::SUBMIT_REVIEW, review, object(&[("id", "integer")], &[]), Idempotency::Lookup),
        ActionSpec::write(
            code::RESOLVE_THREAD,
            object(&[("thread", "string")], &[("repo", "string")]),
            object(&[("resolved", "boolean")], &[]),
            Idempotency::Native,
        ),
        ActionSpec::write(
            code::MERGE,
            merge,
            object(&[("merged", "boolean")], &[("sha", "string"), ("already", "boolean")]),
            Idempotency::Lookup,
        ),
    ]
}

pub fn pipelines() -> Vec<ActionSpec> {
    let run_ref = object(&[("repo", "string"), ("run", "integer")], &[]);
    let review = object(
        &[("repo", "string"), ("run", "integer"), ("decision", "string")],
        &[("environments", "array"), ("comment", "string")],
    );
    vec![
        ActionSpec::read(
            pipelines::RUNS,
            object(&[("repo", "string")], &[("ref", "string"), ("workflow", "string"), ("limit", "integer")]),
            list("runs"),
        ),
        ActionSpec::read(pipelines::RUN, run_ref.clone(), object(&[("id", "integer"), ("jobs", "array")], &[])),
        ActionSpec::read(
            pipelines::JOB_LOG,
            object(&[("repo", "string"), ("job", "integer")], &[("tail", "integer")]),
            object(&[("log", "string")], &[]),
        ),
        ActionSpec::read(pipelines::PENDING_DEPLOYMENTS, run_ref, list("deployments")),
        ActionSpec::write(
            pipelines::REVIEW_DEPLOYMENT,
            review,
            object(&[("reviewed", "array"), ("already", "boolean")], &[]),
            Idempotency::Lookup,
        ),
    ]
}

pub fn cloud() -> Vec<ActionSpec> {
    let credentials = object(
        &[("account", "string"), ("role", "string"), ("access_key_id", "string"), ("secret_access_key", "string")],
        &[("session_token", "string"), ("expires_at", "string"), ("region", "string")],
    );
    vec![
        ActionSpec::read(cloud::IDENTITY, object(&[("account", "string"), ("role", "string")], &[]), credentials),
        ActionSpec::read(cloud::ACCOUNTS_AND_ROLES, json!({ "type": "object", "properties": {} }), list("accounts")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_standard_action_is_named_after_its_capability() {
        for (capability, actions) in [(TRACKER, tracker()), (CODE, code()), (PIPELINES, pipelines()), (CLOUD, cloud())]
        {
            assert!(!actions.is_empty());
            for action in actions {
                assert_eq!(action.capability().map(|c| c.0), Some(capability.to_owned()), "{}", action.name);
                assert_eq!(action.input_schema["type"], "object");
            }
        }
    }

    #[test]
    fn writes_declare_how_they_stay_idempotent() {
        let all: Vec<ActionSpec> = [tracker(), code(), pipelines(), cloud()].concat();
        let lookups: Vec<&str> =
            all.iter().filter(|a| a.idempotent == Idempotency::Lookup).map(|a| a.name.as_str()).collect();
        assert!(lookups.contains(&tracker::CREATE_ISSUE));
        assert!(lookups.contains(&code::MERGE));
        assert!(all.iter().filter(|a| !a.writes).all(|a| a.idempotent == Idempotency::Native));
    }
}
