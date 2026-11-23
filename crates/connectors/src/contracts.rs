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

fn list() -> Value {
    json!({ "type": "array" })
}

fn issue_output() -> Value {
    object(
        &[("ref", "string"), ("title", "string"), ("status", "string"), ("url", "string")],
        &[
            ("description", "string"),
            ("impediment", "object"),
            ("subtasks", "array"),
            ("attachments", "array"),
            ("comments", "array"),
        ],
    )
}

fn issue_summary() -> Value {
    object(&[("ref", "string"), ("status", "string")], &[("title", "string"), ("url", "string"), ("changed", "boolean")])
}

pub fn tracker() -> Vec<ActionSpec> {
    let new_issue = object(
        &[("title", "string")],
        &[("project", "string"), ("description", "string"), ("type", "string"), ("parent", "string"), ("labels", "array")],
    );
    vec![
        ActionSpec::write(tracker::CREATE_ISSUE, new_issue, issue_summary(), Idempotency::Lookup),
        ActionSpec::read(tracker::ISSUE, object(&[("issue", "string")], &[]), issue_output()),
        ActionSpec::write(
            tracker::COMMENT,
            object(&[("issue", "string"), ("body", "string")], &[]),
            object(&[("id", "string"), ("body", "string")], &[("created", "boolean")]),
            Idempotency::Lookup,
        ),
        ActionSpec::read(tracker::STATUSES, object(&[], &[("project", "string")]), list()),
        ActionSpec::write(
            tracker::TRANSITION,
            object(&[("issue", "string"), ("status", "string")], &[]),
            issue_summary(),
            Idempotency::Lookup,
        ),
        ActionSpec::write(
            tracker::SET_IMPEDIMENT,
            object(&[("issue", "string"), ("on", "boolean")], &[("reason", "string")]),
            object(&[("on", "boolean")], &[("reason", "string")]),
            Idempotency::Lookup,
        ),
        ActionSpec::read(
            tracker::SEARCH,
            object(
                &[],
                &[
                    ("query", "string"),
                    ("project", "string"),
                    ("status", "string"),
                    ("limit", "integer"),
                    ("cursor", "string"),
                ],
            ),
            object(&[("issues", "array")], &[("cursor", "string")]),
        ),
    ]
}

fn pull_output() -> Value {
    object(
        &[
            ("ref", "string"),
            ("repo", "string"),
            ("number", "integer"),
            ("state", "string"),
            ("url", "string"),
            ("head", "string"),
            ("base", "string"),
            ("title", "string"),
        ],
        &[("body", "string"), ("created", "boolean"), ("checks", "array"), ("reviews", "array"), ("merged", "boolean")],
    )
}

fn pull_ref() -> Value {
    object(&[("pr", "string")], &[])
}

pub fn code() -> Vec<ActionSpec> {
    let open = object(
        &[("head", "string"), ("title", "string")],
        &[("repo", "string"), ("base", "string"), ("body", "string"), ("draft", "boolean")],
    );
    let review =
        object(&[("pr", "string"), ("verdict", "string")], &[("body", "string"), ("lines", "array")]);
    let merge = object(&[("pr", "string")], &[("method", "string")]);
    vec![
        ActionSpec::write(code::OPEN_PULL_REQUEST, open, pull_output(), Idempotency::Lookup),
        ActionSpec::read(code::PULL_REQUEST, pull_ref(), pull_output()),
        ActionSpec::read(code::FILES_DIFF, pull_ref(), list()),
        ActionSpec::read(code::REVIEW_COMMENTS, pull_ref(), list()),
        ActionSpec::write(
            code::SUBMIT_REVIEW,
            review,
            object(&[("id", "string"), ("verdict", "string")], &[("created", "boolean")]),
            Idempotency::Lookup,
        ),
        ActionSpec::write(
            code::RESOLVE_THREAD,
            object(&[("thread", "string")], &[("pr", "string")]),
            object(&[("thread", "string"), ("resolved", "boolean")], &[]),
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

fn run_output() -> Value {
    object(
        &[("ref", "string"), ("state", "string")],
        &[("pipeline", "string"), ("head", "string"), ("url", "string"), ("jobs", "array")],
    )
}

pub fn pipelines() -> Vec<ActionSpec> {
    let run_ref = object(&[("run", "string")], &[]);
    let review = object(
        &[("run", "string"), ("decision", "string")],
        &[("deployments", "array"), ("comment", "string")],
    );
    vec![
        ActionSpec::read(
            pipelines::RUNS,
            object(&[("ref", "string")], &[("repo", "string"), ("pipeline", "string"), ("limit", "integer")]),
            list(),
        ),
        ActionSpec::read(pipelines::RUN, run_ref.clone(), run_output()),
        ActionSpec::read(
            pipelines::JOB_LOG,
            object(&[("run", "string"), ("job", "string")], &[("tail", "integer")]),
            object(&[("text", "string")], &[("truncated", "boolean")]),
        ),
        ActionSpec::read(pipelines::PENDING_DEPLOYMENTS, run_ref, list()),
        ActionSpec::write(
            pipelines::REVIEW_DEPLOYMENT,
            review,
            object(&[("decision", "string"), ("deployments", "array")], &[("already", "boolean")]),
            Idempotency::Lookup,
        ),
    ]
}

pub fn cloud() -> Vec<ActionSpec> {
    let credentials = object(
        &[("account", "string"), ("access_key_id", "string"), ("secret_access_key", "string")],
        &[
            ("role", "string"),
            ("arn", "string"),
            ("session_token", "string"),
            ("expires_at", "string"),
            ("region", "string"),
        ],
    );
    vec![
        ActionSpec::read(cloud::IDENTITY, object(&[], &[("account", "string"), ("role", "string")]), credentials),
        ActionSpec::read(cloud::ACCOUNTS_AND_ROLES, json!({ "type": "object", "properties": {} }), object(&[("accounts", "array")], &[])),
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
