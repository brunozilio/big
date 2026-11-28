use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::Value;

use crate::descriptor::ActionSpec;

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

pub const CATALOG: &str = "sdk/connectors/contracts.ts";

pub(crate) fn sdk(root: Option<&Path>, export: &str, arguments: &[Value]) -> Result<Value, String> {
    let mine = content::user_text(root, CATALOG).filter(|source| script::exports(CATALOG, source, export));
    let source = mine.or_else(|| content::builtin_text(CATALOG)).ok_or_else(|| format!("{CATALOG} is missing"))?;
    let host: Arc<dyn script::Host> = Arc::new(crate::scripted::Local);
    script::run_within(CATALOG, &source, export, arguments, &Value::Null, host, Arc::new(AtomicBool::new(false)), None)
}

fn catalog(capability: &str) -> Vec<ActionSpec> {
    let listed = sdk(content::folder().as_deref(), capability, &[])
        .and_then(|listed| serde_json::from_value(listed).map_err(|error| error.to_string()));
    listed.unwrap_or_else(|error| {
        tracing::error!(%error, capability, "the action catalog of the sdk does not read");
        Vec::new()
    })
}

pub fn tracker() -> Vec<ActionSpec> {
    catalog(TRACKER)
}

pub fn code() -> Vec<ActionSpec> {
    catalog(CODE)
}

pub fn pipelines() -> Vec<ActionSpec> {
    catalog(PIPELINES)
}

pub fn cloud() -> Vec<ActionSpec> {
    catalog(CLOUD)
}

pub fn changes() -> Vec<ActionSpec> {
    catalog(CHANGES)
}

pub fn tests() -> Vec<ActionSpec> {
    catalog(TESTS)
}

pub fn security() -> Vec<ActionSpec> {
    catalog(SECURITY)
}

pub fn quality() -> Vec<ActionSpec> {
    catalog(QUALITY)
}

pub fn mcp() -> Vec<ActionSpec> {
    catalog(MCP)
}

#[cfg(test)]
mod checks {
    use super::*;
    use crate::descriptor::Idempotency;

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
