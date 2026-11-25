use super::*;
use serde_json::json;

#[test]
fn credits_trial_and_bonus_each_become_a_window_with_their_own_reset() {
    let value = json!({
        "daysUntilReset": 7,
        "nextDateReset": 1_795_456_800.0,
        "subscriptionInfo": {"subscriptionTitle": "KIRO PRO+", "type": "Q_DEVELOPER_STANDALONE_PRO_PLUS"},
        "usageBreakdownList": [{
            "resourceType": "CREDIT",
            "displayName": "Credit",
            "displayNamePlural": "Credits",
            "currentUsage": 412,
            "currentUsageWithPrecision": 412.5,
            "usageLimit": 2000,
            "usageLimitWithPrecision": 2000.0,
            "freeTrialInfo": {"freeTrialStatus": "ACTIVE", "currentUsageWithPrecision": 50.0, "usageLimitWithPrecision": 500.0, "freeTrialExpiry": 1_795_888_800},
            "bonuses": [
                {"displayName": "Welcome bonus", "currentUsage": 10, "usageLimit": 100, "expiresAt": "2026-12-01T00:00:00Z", "status": "ACTIVE"},
                {"displayName": "Old bonus", "currentUsage": 1, "usageLimit": 10, "status": "EXPIRED"}
            ]
        }]
    });
    let usage = windows(&value, domain::Language::Portuguese).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(usage.plan.as_deref(), Some("KIRO PRO+"));
    let shown: Vec<_> =
        usage.windows.iter().map(|w| (w.id.as_str(), w.label.as_str(), w.used_percent, w.resets_at.clone())).collect();
    assert_eq!(
        shown,
        [
            ("credit", "Credits · 412.5 de 2000", 20.625, Some("2026-11-23T18:00:00Z".to_owned())),
            ("credit_trial", "Credits (teste gratuito) · 50 de 500", 10.0, Some("2026-11-28T18:00:00Z".to_owned())),
            ("credit_bonus_0", "Welcome bonus · 10 de 100", 10.0, Some("2026-12-01T00:00:00Z".to_owned())),
        ]
    );
}

#[test]
fn a_breakdown_without_a_limit_is_skipped_and_the_plan_falls_back_to_the_type() {
    let value = json!({
        "subscriptionInfo": {"type": "Q_DEVELOPER_STANDALONE_FREE"},
        "usageBreakdownList": [
            {"resourceType": "AGENTIC_REQUEST", "currentUsage": 3, "usageLimit": 0},
            {"resourceType": "AGENTIC_REQUEST", "displayNamePlural": "Requests", "currentUsage": 30, "usageLimit": 50, "nextDateReset": "2026-12-01T00:00:00Z"}
        ]
    });
    let usage = windows(&value, domain::Language::Portuguese).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(usage.plan.as_deref(), Some("Q DEVELOPER STANDALONE FREE"));
    assert_eq!(usage.windows.len(), 1);
    assert_eq!(usage.windows[0].label, "Requests · 30 de 50");
    assert!((usage.windows[0].used_percent - 60.0).abs() < f64::EPSILON);
}

#[test]
fn an_answer_without_a_breakdown_is_an_error() {
    assert!(windows(&json!({"message": "nope"}), domain::Language::Portuguese).is_err());
}

#[test]
fn the_call_goes_to_the_profile_region_with_the_usage_target() {
    let config: Config = serde_json::from_value(json!({
        "auth": {"access_token": "tok", "refresh_token": "r", "account_id": "arn:aws:codewhisperer:eu-central-1:1:profile/X"}
    }))
    .unwrap_or_else(|e| panic!("{e}"));
    let call = call(&config);
    assert_eq!(call.url, "https://q.eu-central-1.amazonaws.com/");
    assert!(call.headers.iter().any(|(name, value)| name == "x-amz-target" && value == TARGET));
    let body: Value = serde_json::from_str(call.body.as_deref().unwrap_or_default()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(body["profileArn"], "arn:aws:codewhisperer:eu-central-1:1:profile/X");
}
