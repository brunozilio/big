use std::path::{Path, PathBuf};
use std::sync::Arc;

use platform::sync::Locked;

use super::{Models, Policy, Tracking, Workspace};
use crate::tools::read_files::Freshness;
use crate::tools::script_tool::ScriptGrants;

#[test]
fn a_forked_policy_keeps_its_settings_and_scripts() {
    let policy = Policy {
        scripts: ScriptGrants::none(),
        redact: Some(Arc::new(domain::redact::Known::default())),
        escalation: true,
        auto_review: true,
        follow_through: true,
        affected_tests: true,
        session_claims: true,
        load_on_call: true,
        code_mode: domain::code_mode::Mode::Only,
        code_mode_detach: Some(std::time::Duration::from_secs(5)),
        web: domain::web_reading::WebReading::none(),
        output: domain::output::Shaping { cleanup: true, filters: false, savings: true },
        bash_output: domain::output::OutputLimit::of(4_000),
    };

    let fork = policy.fork();

    assert_eq!(fork.web, policy.web, "a sub-agent reads the web as its parent does");
    assert_eq!(fork.output, policy.output, "a sub-agent's command output is shaped as its parent's");
    assert_eq!(fork.bash_output, policy.bash_output, "a sub-agent's command output is cut as its parent's");
    assert!(fork.redact.is_some(), "a sub-agent's output is redacted like its parent's");
    assert!(fork.load_on_call, "a sub-agent loads a waiting tool on call as its parent does");
    assert_eq!(fork.code_mode, policy.code_mode, "a sub-agent scripts its tools as its parent does");
    assert_eq!(fork.scripts, ScriptGrants::none());
}

#[test]
fn a_forked_tracking_shares_what_was_read() {
    let tracking = Tracking::default();
    let fork = tracking.fork();
    let path = PathBuf::from("/repo/missing-on-purpose.rs");

    fork.mark_read(&path);

    assert!(Arc::ptr_eq(&tracking.read, &fork.read));
    assert_ne!(tracking.freshness(&path), Freshness::Unread);
}

#[test]
fn a_forked_tracking_starts_with_no_refusals_seen_files_or_seen_rules() {
    let tracking = Tracking::default();
    for _ in 0..domain::interception::MAX_REFUSALS_IN_A_ROW {
        tracking.count_refusal(true);
    }
    tracking.seen_rules.locked().insert(PathBuf::from("/repo/.rules/a.md"));

    let fork = tracking.fork();

    assert!(tracking.refusals_stop().is_some());
    assert_eq!(fork.refusals_stop(), None);
    assert!(fork.seen_rules.locked().is_empty());
    assert!(!Arc::ptr_eq(&tracking.seen_rules, &fork.seen_rules));
    assert!(!Arc::ptr_eq(&tracking.seen_files, &fork.seen_files));
}

#[test]
fn a_forked_workspace_keeps_models_and_drops_the_worktree() {
    let mut workspace = Workspace::new("/repo");
    workspace.models = Models { small_model: Some("small".into()), ..Models::default() };

    let fork = workspace.fork(Arc::default());

    assert_eq!(fork.models.small_model.as_deref(), Some("small"));
    assert_eq!(fork.cwd, Path::new("/repo"));
    assert!(fork.worktree.is_none());
}
