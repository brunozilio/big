use super::*;
use serde_json::json;

#[test]
fn limited_quotas_are_used_percent_and_unlimited_ones_are_skipped() {
    let value = json!({
        "copilot_plan": "individual_pro",
        "quota_reset_date": "2026-12-01",
        "quota_snapshots": {
            "premium_interactions": {"entitlement": 300, "percent_remaining": 62.5, "unlimited": false},
            "chat": {"entitlement": 0, "percent_remaining": 100, "unlimited": true},
            "completions": {"percent_remaining": 100.0, "unlimited": false}
        }
    });
    let usage = windows(&value).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(usage.plan.as_deref(), Some("Individual pro"));
    let shown: Vec<_> =
        usage.windows.iter().map(|w| (w.id.as_str(), w.label.as_str(), w.used_percent, w.resets_at.clone())).collect();
    let reset = Some("2026-12-01T00:00:00Z".to_owned());
    assert_eq!(
        shown,
        [
            ("premium_interactions", "Requisições premium", 37.5, reset.clone()),
            ("completions", "Autocompletar", 0.0, reset),
        ]
    );
}

#[test]
fn the_utc_reset_wins_and_a_user_without_quotas_has_no_windows() {
    let value = json!({"quota_reset_date_utc": "2026-12-01T00:00:00.000Z", "quota_reset_date": "2026-11-30"});
    let usage = windows(&value).unwrap_or_else(|e| panic!("{e}"));
    assert!(usage.windows.is_empty() && usage.plan.is_none());
    assert!(windows(&json!("x")).is_err());
}

#[test]
fn the_call_goes_to_the_github_user_with_the_github_token() {
    let call = call(&Config { auth: Some(llm_providers::Tokens::bearer("gho_x")), ..Config::default() });
    assert_eq!(call.url, device::USER);
    assert!(call.headers.contains(&("authorization".to_owned(), "Bearer gho_x".to_owned())));
}
