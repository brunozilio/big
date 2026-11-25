use super::*;
use serde_json::json;

#[test]
fn each_window_with_a_utilization_is_kept_and_nulls_are_skipped() {
    let value = json!({
        "five_hour": {"utilization": 42.0, "resets_at": "2026-11-23T18:00:00.123+00:00"},
        "seven_day": {"utilization": 10, "resets_at": null},
        "seven_day_opus": null,
        "seven_day_sonnet": {"utilization": null, "resets_at": "2026-11-28T00:00:00+00:00"},
        "extra_usage": {"is_enabled": false}
    });
    let usage = windows(&value, domain::Language::Portuguese).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(usage.plan, None);
    let shown: Vec<_> =
        usage.windows.iter().map(|w| (w.id.as_str(), w.label.as_str(), w.used_percent, w.resets_at.clone())).collect();
    assert_eq!(
        shown,
        [
            ("five_hour", "Sessão (5 h)", 42.0, Some("2026-11-23T18:00:00.123Z".to_owned())),
            ("seven_day", "Semana", 10.0, None),
        ]
    );
}

#[test]
fn a_body_that_is_not_an_object_is_refused() {
    assert!(windows(&json!([]), domain::Language::Portuguese).is_err());
}

#[test]
fn the_call_asks_for_json_with_the_subscription_token() {
    let tokens = llm_providers::Tokens { refresh_token: "r".into(), ..llm_providers::Tokens::bearer("a") };
    let config = Config { auth: Some(tokens), ..Config::default() };
    let call = call(&super::super::Provider, &config);
    assert_eq!(call.url, USAGE);
    assert!(call.headers.contains(&("authorization".to_owned(), "Bearer a".to_owned())));
    assert!(call.headers.contains(&("accept".to_owned(), "application/json".to_owned())));
    assert!(call.headers.iter().any(|(name, value)| name == "anthropic-beta" && value.contains("oauth-2025-04-20")));
}

#[test]
fn the_client_asks_the_usage_once_and_a_provider_without_limits_asks_nothing() {
    use crate::providers::testing::{client, header};
    let answer = json!({"five_hour": {"utilization": 7, "resets_at": null}}).to_string();
    let (claude, http) = client(domain::ModelProvider::new("claude-subscription"), [(200, answer)]);
    let usage = claude
        .limits(domain::Language::Portuguese)
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|| panic!("supported"));
    assert_eq!(usage.windows.len(), 1);
    let sent = http.sent();
    assert_eq!((sent.len(), sent[0].url.as_str()), (1, USAGE));
    assert_eq!(header(&sent[0], "authorization").as_deref(), Some("Bearer access-1"));
    let (grok, http) = client(domain::ModelProvider::new("grok-subscription"), []);
    assert_eq!(grok.limits(domain::Language::Portuguese).ok(), Some(None));
    assert!(http.sent().is_empty());
    let (failing, _) = client(domain::ModelProvider::new("claude-subscription"), [(401, "{}".to_owned())]);
    assert!(failing.limits(domain::Language::Portuguese).is_err());
}
