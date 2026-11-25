use super::*;
use serde_json::json;

#[test]
fn both_windows_are_named_by_their_length_and_reset_at_their_epoch() {
    let value = json!({
        "plan_type": "pro",
        "rate_limit": {
            "allowed": true,
            "primary_window": {"used_percent": 42, "limit_window_seconds": 18000, "reset_after_seconds": 60, "reset_at": 1_795_456_800},
            "secondary_window": {"used_percent": 10.5, "limit_window_seconds": 604_800, "reset_at": 1_795_888_800}
        }
    });
    let usage = windows(&value, domain::Language::Portuguese).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(usage.plan.as_deref(), Some("Pro"));
    let shown: Vec<_> =
        usage.windows.iter().map(|w| (w.id.as_str(), w.label.as_str(), w.used_percent, w.resets_at.clone())).collect();
    assert_eq!(
        shown,
        [
            ("primary", "Sessão (5 h)", 42.0, Some("2026-11-23T18:00:00Z".to_owned())),
            ("secondary", "Semana", 10.5, Some("2026-11-28T18:00:00Z".to_owned())),
        ]
    );
}

#[test]
fn a_missing_window_is_skipped_and_a_relative_reset_still_gives_an_instant() {
    let value = json!({"plan_type": null, "rate_limit": {
        "primary_window": {"used_percent": 5, "limit_window_seconds": 10_800, "reset_after_seconds": 30},
        "secondary_window": null
    }});
    let usage = windows(&value, domain::Language::Portuguese).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(usage.plan, None);
    assert_eq!(usage.windows.len(), 1);
    assert_eq!(usage.windows[0].label, "3 h");
    assert!(usage.windows[0].resets_at.is_some());
}

#[test]
fn the_call_carries_the_account_and_asks_for_json() {
    let tokens = llm_providers::Tokens { account_id: "acct".into(), ..llm_providers::Tokens::bearer("a") };
    let call = call(&super::super::Provider, &Config { auth: Some(tokens), ..Config::default() });
    assert_eq!(call.url, "https://chatgpt.com/backend-api/wham/usage");
    assert!(call.headers.contains(&("chatgpt-account-id".to_owned(), "acct".to_owned())));
    assert!(call.headers.contains(&("accept".to_owned(), "application/json".to_owned())));
}
