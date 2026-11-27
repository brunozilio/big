use serde_json::{Value, json};

use crate::descriptor::{ActionSpec, Idempotency, object};

pub const TRACKER: &str = "tracker";
pub const CODE: &str = "code";
pub const PIPELINES: &str = "pipelines";
pub const CLOUD: &str = "cloud";
pub const TESTS: &str = "tests";
pub const MCP: &str = "mcp";
pub const SECURITY: &str = "security";
pub const QUALITY: &str = "quality";
pub const CHANGES: &str = "changes";

pub mod tracker {
    pub const CREATE_ISSUE: &str = "tracker.create_issue";
    pub const ISSUE: &str = "tracker.issue";
    pub const COMMENT: &str = "tracker.comment";
    pub const STATUSES: &str = "tracker.statuses";
    pub const TRANSITION: &str = "tracker.transition";
    pub const SET_IMPEDIMENT: &str = "tracker.set_impediment";
    pub const SEARCH: &str = "tracker.search";
    pub const PROJECTS: &str = "tracker.projects";
    pub const FIELDS: &str = "tracker.fields";
    pub const UPDATE_ISSUE: &str = "tracker.update_issue";
    pub const LINK_ISSUES: &str = "tracker.link_issues";
    pub const ATTACH: &str = "tracker.attach";
    pub const SCHEMA: &str = "tracker.schema";
}

pub mod code {
    pub const OPEN_PULL_REQUEST: &str = "code.open_pull_request";
    pub const PULL_REQUEST: &str = "code.pull_request";
    pub const FILES_DIFF: &str = "code.files_diff";
    pub const REVIEW_COMMENTS: &str = "code.review_comments";
    pub const SUBMIT_REVIEW: &str = "code.submit_review";
    pub const RESOLVE_THREAD: &str = "code.resolve_thread";
    pub const MERGE: &str = "code.merge";
    pub const REPOS: &str = "code.repos";
    pub const PULL_REQUESTS: &str = "code.pull_requests";
    pub const BRANCHES: &str = "code.branches";
    pub const MERGE_BRANCHES: &str = "code.merge_branches";
    pub const GIT_CREDENTIALS: &str = "code.git_credentials";
}

pub mod pipelines {
    pub const RUNS: &str = "pipelines.runs";
    pub const RUN: &str = "pipelines.run";
    pub const JOB_LOG: &str = "pipelines.job_log";
    pub const PENDING_DEPLOYMENTS: &str = "pipelines.pending_deployments";
    pub const REVIEW_DEPLOYMENT: &str = "pipelines.review_deployment";
}

pub const OPTIONS: &str = "options";

pub fn options_of(capability: &str) -> String {
    format!("{capability}.{OPTIONS}")
}

pub mod cloud {
    pub const IDENTITY: &str = "cloud.identity";
    pub const ACCOUNTS_AND_ROLES: &str = "cloud.accounts_and_roles";
    pub const LAST_DEPLOYMENT: &str = "cloud.last_deployment";
}

pub mod changes {
    pub const CREATE: &str = "changes.create";
    pub const UPDATE: &str = "changes.update";
    pub const GET: &str = "changes.get";
    pub const CANCEL: &str = "changes.cancel";
    pub const STATUS_CHANGED: &str = "changes.status.changed";
}

pub mod tests {
    pub const CREATE_CASE: &str = "tests.create_case";
    pub const CASES: &str = "tests.cases";
    pub const CREATE_CYCLE: &str = "tests.create_cycle";
    pub const RECORD: &str = "tests.record";
    pub const FINISH_CYCLE: &str = "tests.finish_cycle";
    pub const PROJECTS: &str = "tests.projects";
}

pub mod security {
    pub const PROJECTS: &str = "security.projects";
    pub const FINDINGS: &str = "security.findings";
    pub const STATUS: &str = "security.status";
}

pub mod quality {
    pub const REPORT: &str = "quality.report";
}

pub mod mcp {
    pub const TOOLS: &str = "mcp.tools";
    pub const URL: &str = "url";
}

fn list() -> Value {
    json!({ "type": "array" })
}

fn issue_output() -> Value {
    object(
        &[("ref", "string"), ("title", "string"), ("status", "string"), ("url", "string")],
        &[
            ("id", "string"),
            ("project", "string"),
            ("type", "string"),
            ("parent", "string"),
            ("description", "string"),
            ("impediment", "object"),
            ("subtasks", "array"),
            ("attachments", "array"),
            ("comments", "array"),
        ],
    )
}

fn issue_summary() -> Value {
    object(
        &[("ref", "string"), ("status", "string")],
        &[("id", "string"), ("title", "string"), ("url", "string"), ("changed", "boolean")],
    )
}

fn project_list() -> Value {
    let project = object(&[("id", "string"), ("key", "string"), ("name", "string")], &[("avatar", "string")]);
    json!({ "type": "array", "items": project })
}

fn field_list() -> Value {
    let field = object(
        &[("key", "string"), ("name", "string"), ("type", "string")],
        &[("required", "boolean"), ("values", "array")],
    );
    json!({ "type": "array", "items": field })
}

fn tracker_schema() -> Value {
    let status = object(&[("name", "string")], &[("id", "string"), ("category", "string")]);
    let mut kind = object(
        &[("name", "string"), ("level", "integer"), ("children", "array"), ("statuses", "array")],
        &[("id", "string")],
    );
    kind["properties"]["statuses"] = json!({ "type": "array", "items": status });
    let mut shaped = object(&[("types", "array")], &[("default", "string")]);
    shaped["properties"]["types"] = json!({ "type": "array", "items": kind });
    shaped
}

pub fn tracker() -> Vec<ActionSpec> {
    let new_issue = object(
        &[("title", "string")],
        &[
            ("project", "string"),
            ("description", "string"),
            ("type", "string"),
            ("parent", "string"),
            ("labels", "array"),
            ("fields", "object"),
        ],
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
            object(&[("issue", "string")], &[("status", "string"), ("category", "string")]),
            issue_summary(),
            Idempotency::Lookup,
        ),
        ActionSpec::read(tracker::SCHEMA, object(&[], &[("project", "string")]), tracker_schema()),
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
        ActionSpec::read(tracker::PROJECTS, object(&[], &[("query", "string")]), project_list()),
        ActionSpec::read(tracker::FIELDS, object(&[], &[("project", "string"), ("type", "string")]), field_list()),
        ActionSpec::write(
            tracker::UPDATE_ISSUE,
            object(&[("issue", "string")], &[("title", "string"), ("description", "string"), ("fields", "object")]),
            issue_summary(),
            Idempotency::Native,
        ),
        ActionSpec::write(
            tracker::LINK_ISSUES,
            object(&[("issue", "string"), ("to", "string")], &[("kind", "string")]),
            object(&[("issue", "string"), ("to", "string")], &[("already", "boolean")]),
            Idempotency::Lookup,
        ),
        ActionSpec::write(
            tracker::ATTACH,
            object(&[("issue", "string"), ("files", "array")], &[]),
            object(&[("attached", "array")], &[("skipped", "array")]),
            Idempotency::Lookup,
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

fn repo_list() -> Value {
    let repo = object(
        &[("id", "string"), ("ref", "string"), ("name", "string")],
        &[("url", "string"), ("default_branch", "string"), ("private", "boolean")],
    );
    json!({ "type": "array", "items": repo })
}

fn branch_list() -> Value {
    let branch = object(&[("name", "string")], &[("default", "boolean")]);
    json!({ "type": "array", "items": branch })
}

pub fn code() -> Vec<ActionSpec> {
    let open = object(
        &[("head", "string"), ("title", "string")],
        &[("repo", "string"), ("base", "string"), ("body", "string"), ("draft", "boolean")],
    );
    let review = object(
        &[("pr", "string"), ("verdict", "string")],
        &[("body", "string"), ("lines", "array"), ("agent", "string")],
    );
    let merge = object(&[("pr", "string")], &[("method", "string"), ("delete_branch", "boolean")]);
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
            object(&[("merged", "boolean")], &[("sha", "string"), ("already", "boolean"), ("deleted", "boolean")]),
            Idempotency::Lookup,
        ),
        ActionSpec::read(code::REPOS, object(&[], &[("query", "string")]), repo_list()),
        ActionSpec::read(code::PULL_REQUESTS, object(&[], &[("repo", "string")]), list()),
        ActionSpec::read(code::BRANCHES, object(&[], &[("repo", "string")]), branch_list()),
        ActionSpec::write(
            code::MERGE_BRANCHES,
            object(&[("base", "string"), ("head", "string")], &[("repo", "string"), ("message", "string")]),
            object(&[("merged", "boolean")], &[("sha", "string"), ("already", "boolean")]),
            Idempotency::Lookup,
        ),
        ActionSpec::read(
            code::GIT_CREDENTIALS,
            object(&[("url", "string")], &[]),
            object(&[("username", "string"), ("password", "string")], &[]),
        )
        .secret(),
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
    let review =
        object(&[("run", "string"), ("decision", "string")], &[("deployments", "array"), ("comment", "string")]);
    vec![
        ActionSpec::read(
            pipelines::RUNS,
            object(
                &[],
                &[
                    ("ref", "string"),
                    ("repo", "string"),
                    ("pipeline", "string"),
                    ("limit", "integer"),
                    ("status", "string"),
                ],
            ),
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
        ActionSpec::read(
            cloud::IDENTITY,
            object(
                &[],
                &[
                    ("account", "string"),
                    ("role", "string"),
                    ("binding", "string"),
                    ("profiles", "array"),
                    ("environment", "string"),
                ],
            ),
            credentials,
        )
        .secret(),
        ActionSpec::read(
            cloud::ACCOUNTS_AND_ROLES,
            json!({ "type": "object", "properties": {} }),
            object(&[("accounts", "array")], &[]),
        ),
        ActionSpec::read(
            cloud::LAST_DEPLOYMENT,
            object(&[("service", "string"), ("environment", "string")], &[("account", "string")]),
            object(&[("version", "string")], &[("at", "string"), ("source", "string")]),
        ),
    ]
}

fn change_output() -> Value {
    object(
        &[("ref", "string"), ("key", "string"), ("status", "string")],
        &[
            ("title", "string"),
            ("environment", "string"),
            ("start", "string"),
            ("end", "string"),
            ("url", "string"),
            ("revision", "integer"),
            ("created", "boolean"),
        ],
    )
}

pub fn changes() -> Vec<ActionSpec> {
    let fields = [
        ("settings", "object"),
        ("tasks", "array"),
        ("notes", "array"),
        ("attachments", "array"),
        ("versions", "array"),
        ("description", "string"),
    ];
    let mut update_fields = vec![("title", "string"), ("start", "string"), ("end", "string")];
    update_fields.extend(fields);
    let change_ref = object(&[("change", "string")], &[]);
    let option = object(&[("value", "string"), ("label", "string")], &[("detail", "string")]);
    vec![
        ActionSpec::write(
            changes::CREATE,
            object(&[("title", "string"), ("environment", "string"), ("start", "string"), ("end", "string")], &fields),
            change_output(),
            Idempotency::Lookup,
        ),
        ActionSpec::write(
            changes::UPDATE,
            object(&[("change", "string")], &update_fields),
            change_output(),
            Idempotency::Native,
        ),
        ActionSpec::read(changes::GET, change_ref.clone(), change_output()),
        ActionSpec::write(changes::CANCEL, change_ref, change_output(), Idempotency::Native),
        ActionSpec::read(
            &options_of(CHANGES),
            object(&[], &[("query", "string"), ("field", "string"), ("chosen", "object"), ("page", "integer")]),
            json!({ "type": "array", "items": option }),
        ),
    ]
}

fn with_issue_id(mut schema: Value) -> Value {
    schema["properties"]["issue_id"] = json!({ "type": ["string", "integer"] });
    schema
}

pub fn tests() -> Vec<ActionSpec> {
    let new_case = with_issue_id(object(
        &[("title", "string")],
        &[
            ("project", "string"),
            ("objective", "string"),
            ("precondition", "string"),
            ("labels", "array"),
            ("steps", "array"),
            ("folder", "string"),
            ("issue", "string"),
            ("issue_id", "string"),
        ],
    ));
    let new_cycle = with_issue_id(object(
        &[("title", "string")],
        &[("project", "string"), ("description", "string"), ("issue", "string"), ("issue_id", "string")],
    ));
    let record = with_issue_id(object(
        &[("case", "string"), ("cycle", "string"), ("status", "string")],
        &[("project", "string"), ("comment", "string"), ("seconds", "number"), ("issue_id", "string")],
    ));
    let finish = object(&[("cycle", "string"), ("status", "string")], &[("project", "string")]);
    let case = object(&[("ref", "string"), ("title", "string")], &[]);
    vec![
        ActionSpec::write(
            tests::CREATE_CASE,
            new_case,
            object(&[("ref", "string"), ("title", "string"), ("url", "string")], &[("created", "boolean")]),
            Idempotency::Lookup,
        ),
        ActionSpec::read(tests::CASES, object(&[("issue", "string")], &[]), json!({ "type": "array", "items": case })),
        ActionSpec::write(
            tests::CREATE_CYCLE,
            new_cycle,
            object(&[("ref", "string"), ("url", "string")], &[]),
            Idempotency::Native,
        ),
        ActionSpec::write(tests::RECORD, record, object(&[("ref", "string")], &[]), Idempotency::Native),
        ActionSpec::write(
            tests::FINISH_CYCLE,
            finish,
            object(&[("ref", "string")], &[("status", "string")]),
            Idempotency::Native,
        ),
        ActionSpec::read(tests::PROJECTS, object(&[], &[("query", "string")]), project_list()),
    ]
}

fn finding_list() -> Value {
    let finding = object(
        &[
            ("id", "string"),
            ("title", "string"),
            ("severity", "integer"),
            ("kind", "string"),
            ("status", "string"),
            ("new", "boolean"),
            ("policy", "boolean"),
            ("description", "string"),
        ],
        &[
            ("cwe", "object"),
            ("cve", "object"),
            ("file", "string"),
            ("line", "integer"),
            ("module", "string"),
            ("component", "string"),
            ("version", "string"),
            ("found", "string"),
            ("url", "string"),
        ],
    );
    json!({ "type": "array", "items": finding })
}

pub fn security() -> Vec<ActionSpec> {
    let wanted = object(
        &[],
        &[
            ("project", "string"),
            ("kinds", "array"),
            ("minSeverity", "integer"),
            ("policy", "boolean"),
            ("open", "boolean"),
            ("limit", "integer"),
        ],
    );
    vec![
        ActionSpec::read(security::PROJECTS, object(&[], &[("query", "string")]), project_list()),
        ActionSpec::read(security::FINDINGS, wanted, finding_list()),
    ]
}

pub fn quality() -> Vec<ActionSpec> {
    let wanted =
        object(&[("project", "string")], &[("pullRequest", "string"), ("branch", "string"), ("commit", "string")]);
    let report = object(
        &[("status", "string")],
        &[
            ("gate", "string"),
            ("ratings", "object"),
            ("counts", "object"),
            ("coverage", "number"),
            ("duplication", "number"),
            ("conditions", "array"),
            ("issues", "array"),
            ("total", "integer"),
            ("url", "string"),
            ("analyzedAt", "string"),
        ],
    );
    vec![ActionSpec::read(quality::REPORT, wanted, report)]
}

pub fn mcp() -> Vec<ActionSpec> {
    vec![ActionSpec::read(mcp::TOOLS, object(&[], &[]), list())]
}

#[cfg(test)]
mod checks {
    use super::*;

    #[test]
    fn every_standard_action_is_named_after_its_capability() {
        let all = [
            (TRACKER, tracker()),
            (CODE, code()),
            (PIPELINES, pipelines()),
            (CLOUD, cloud()),
            (TESTS, tests()),
            (SECURITY, security()),
            (QUALITY, quality()),
            (MCP, mcp()),
            (CHANGES, changes()),
        ];
        for (capability, actions) in all {
            assert!(!actions.is_empty());
            for action in actions {
                assert_eq!(action.capability().map(|c| c.0), Some(capability.to_owned()), "{}", action.name);
                assert_eq!(action.input_schema["type"], "object");
            }
        }
    }

    #[test]
    fn writes_declare_how_they_stay_idempotent() {
        let all: Vec<ActionSpec> =
            [tracker(), code(), pipelines(), cloud(), tests(), security(), mcp(), changes()].concat();
        let lookups: Vec<&str> =
            all.iter().filter(|a| a.idempotent == Idempotency::Lookup).map(|a| a.name.as_str()).collect();
        assert!(lookups.contains(&tracker::CREATE_ISSUE));
        assert!(lookups.contains(&code::MERGE));
        assert!(lookups.contains(&tests::CREATE_CASE));
        assert!(lookups.contains(&changes::CREATE));
        assert!(lookups.contains(&code::MERGE_BRANCHES));
        assert!(all.iter().filter(|a| !a.writes).all(|a| a.idempotent == Idempotency::Native));
    }
}
