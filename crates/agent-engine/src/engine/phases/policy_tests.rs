use std::path::{Path, PathBuf};

use domain::code_mode::Mode;
use domain::memory::{Author, Memory, MemoryBrief, MemoryId, MemoryKind};
use domain::{Criteria, ModelOption, PermissionMode};

use super::*;

fn policy() -> Policy {
    flagged(&Settings::default(), Path::new("/nowhere"))
}

fn request() -> PromptRequest {
    PromptRequest::plain(PathBuf::from("/nowhere"), "go", ModelOption::new("m", "M"))
}

type Reads = fn(&Policy) -> bool;

const SWITCHES: [(&str, Reads); 17] = [
    (flags::WEB_LINKS, |policy| policy.web.links),
    (flags::WEB_SMALL_PAGES_DIRECT, |policy| policy.web.small_pages_direct),
    (flags::WEB_FORMATS, |policy| policy.web.formats),
    (flags::WEB_EMPTY_PAGE_ERROR, |policy| policy.web.empty_page_error),
    (flags::REDACT_SECRETS, |policy| policy.redact.is_some()),
    (flags::ESCALATION, |policy| policy.escalation),
    (flags::AUTO_REVIEW, |policy| policy.auto_review),
    (flags::FOLLOW_THROUGH, |policy| policy.follow_through),
    (flags::AFFECTED_TESTS, |policy| policy.affected_tests),
    (flags::SESSION_CLAIMS, |policy| policy.session_claims),
    (flags::LOAD_ON_CALL, |policy| policy.load_on_call),
    (flags::CODE_MODE, |policy| policy.code_mode != Mode::Off),
    (flags::WEB_MAIN_CONTENT, |policy| policy.web.main_content),
    (flags::WEB_CLEANUP, |policy| policy.web.cleanup),
    (flags::OUTPUT_CLEANUP, |policy| policy.output.cleanup),
    (flags::OUTPUT_FILTERS, |policy| policy.output.filters),
    (flags::OUTPUT_SAVINGS, |policy| policy.output.savings),
];

#[test]
fn every_switch_follows_its_flag_in_the_config() {
    let policy = policy();
    for (flag, reads) in SWITCHES {
        assert_eq!(reads(&policy), on(flag));
    }
    assert_eq!(policy.web, domain::web_reading::WebReading::configured());
}

#[test]
fn the_scripts_are_the_ones_the_settings_describe() {
    let settings = Settings::default();
    let policy = flagged(&settings, Path::new("/nowhere"));
    assert_eq!(policy.scripts, settings.scripts);
}

#[test]
fn secrets_are_read_from_the_folder_s_env_files_while_redaction_is_on() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join(".env"), "TOKEN=s3cr3t-value-123456\n").expect("env");
    let said = "the token is s3cr3t-value-123456";
    let redacting = flagged(&Settings::default(), dir.path());
    assert_eq!(redacting.redacted(said).contains("s3cr3t-value-123456"), !on(flags::REDACT_SECRETS));
}

#[test]
fn flags_grant_no_pre_approval() {
    let policy = policy();
    assert!(policy.pre_approved.is_empty());
    assert_eq!(policy.code_mode_detach, None);
}

#[test]
fn a_budget_is_kept_only_while_its_flag_is_on() {
    let mut request = request();
    request.policy.budget = Some(domain::budget::Budget { limit_usd: 2.0, spent_usd: 0.5 });
    let kept = budget(&request);
    assert_eq!(kept, request.policy.budget.filter(|_| on(flags::BUDGET)));
}

#[test]
fn nothing_is_verified_in_plan_mode() {
    let mut request = request();
    request.policy.verification = Some(Criteria::project("tests pass"));
    request.policy.permission = PermissionMode::Plan;
    assert_eq!(verification(&request, &Settings::default(), &ModelRoles::default()), Ok(None));
}

#[test]
fn the_project_s_criteria_apply_only_with_their_flag_and_yield_to_the_request_s() {
    let project = Settings { project_criteria: Some("lint is clean".into()), ..Settings::default() };
    let request = request();
    let judged = verification(&request, &project, &ModelRoles::default()).expect("verification");
    let expected = on(flags::PROJECT_CRITERIA).then(|| Criteria::project("lint is clean"));
    assert_eq!(judged.map(|found| found.criteria), expected);
    let mut asking = request.clone();
    asking.policy.verification = Some(Criteria::project("tests pass"));
    let judged = verification(&asking, &project, &ModelRoles::default()).expect("verification");
    assert_eq!(judged.map(|found| found.criteria.text), Some("tests pass".to_owned()));
}

#[test]
fn the_judge_is_the_review_role_s_model_or_the_run_s_own() {
    let mut request = request();
    request.policy.verification = Some(Criteria::project("tests pass"));
    let own = verification(&request, &Settings::default(), &ModelRoles::default()).expect("verification");
    assert_eq!(own.map(|found| found.model), Some("m".to_owned()));
    let mut roles = ModelRoles::default();
    roles.set(ModelRole::Review, Some("no such model anywhere".into()));
    let error = verification(&request, &Settings::default(), &roles).expect_err("unknown judge");
    assert!(error.starts_with("judgeModel: "), "{error}");
}

fn anchor() -> Memory {
    Memory {
        id: MemoryId(7),
        kind: MemoryKind::Semantic,
        text: "Keep defaultRetryMax at 4".into(),
        tags: Vec::new(),
        created: 0,
        origin: None,
        author: Author::User,
        history: Vec::new(),
        uses: 0,
        last_used: 0,
        touches: vec!["client.go".into()],
        credit: None,
    }
}

#[test]
fn anchored_memories_need_the_brief_to_ask_and_the_flag_to_be_on() {
    let mut request = request();
    let brief = |anchors| MemoryBrief { anchors, anchored: vec![anchor()], ..MemoryBrief::default() };
    request.host.memory = Some(brief(true));
    let expected = if on(flags::MEMORY_ANCHORS) { vec![anchor()] } else { Vec::new() };
    assert_eq!(anchored(&request), expected);
    request.host.memory = Some(brief(false));
    assert_eq!(anchored(&request), []);
}
