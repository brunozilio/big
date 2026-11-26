use domain::{Language, ModelOption};
use jiff::{SignedDuration, Timestamp};
use serde_json::{Value, json};

use super::provider;
use crate::{Config, EFFORTS, Provider, ProviderError, Tokens};

const ARN: &str = "arn:aws:codewhisperer:eu-central-1:699475941385:profile/EHGA3GRVQMUK";
const EAST: &str = "arn:aws:codewhisperer:us-east-1:699475941385:profile/EHGA3GRVQMUK";
const CHAT: &str = "AmazonCodeWhispererStreamingService.GenerateAssistantResponse";
const MODELS: &str = "KiroControlPlaneBearerService.ListAvailableModels";
const USAGE: &str = "AmazonCodeWhispererService.GetUsageLimits";
const REFRESH: &str = "https://prod.us-east-1.auth.desktop.kiro.dev/refreshToken";

fn kiro() -> &'static dyn Provider {
    provider("kiro-subscription")
}

fn setting(key: &str) -> &'static str {
    brand::section("kiro").iter().find(|(name, _)| *name == key).map_or("", |(_, value)| value)
}

fn signed_in(arn: &str) -> Config {
    let tokens = Tokens { refresh_token: "r".into(), account_id: arn.into(), ..Tokens::bearer("a") };
    Config { auth: Some(tokens), ..Config::default() }
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
}

fn listed(value: &Value) -> Vec<ModelOption> {
    kiro().page(value).unwrap_or_else(|e| panic!("{e}")).models
}

fn model(id: &str) -> ModelOption {
    ModelOption::new(format!("kiro-subscription:{id}"), id)
}

fn sent(model: &ModelOption, effort: &str) -> Value {
    let mut body = json!({"conversationState": {}, "output_config": {"effort": effort}});
    kiro().shape(Some(model), &mut body, effort).unwrap_or_else(|e| panic!("{e}"));
    assert!(body.get("output_config").is_none());
    body["additionalModelRequestFields"].clone()
}

fn schema(field: &str, levels: &[&str]) -> Value {
    json!({"properties": {field: {"properties": {"effort": {"enum": levels}}}}})
}

#[test]
fn an_answer_keeps_the_profile_arn_and_its_lifetime() {
    let now = Timestamp::now();
    let body = format!(r#"{{"accessToken":"a","refreshToken":"r","profileArn":"{EAST}","expiresIn":3600}}"#);
    let fresh = kiro().tokens(200, &body, None, now).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((fresh.access_token.as_str(), fresh.refresh_token.as_str()), ("a", "r"));
    assert_eq!(fresh.account_id, EAST);
    assert_eq!(fresh.expires_at, Some(now + SignedDuration::from_secs(3600)));
}

#[test]
fn a_refresh_keeps_what_the_answer_omits_and_failures_sign_out() {
    let previous = Tokens { refresh_token: "r0".into(), account_id: EAST.into(), ..Tokens::bearer("old") };
    let request = kiro().refresh(&previous).unwrap_or_else(|| panic!("refreshable"));
    assert_eq!(request.url, REFRESH);
    assert_eq!(serde_json::from_str::<Value>(&request.body).ok(), Some(json!({"refreshToken": "r0"})));
    let fresh = kiro().tokens(200, r#"{"accessToken":"new"}"#, Some(&previous), Timestamp::now());
    let fresh = fresh.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((fresh.refresh_token.as_str(), fresh.account_id.as_str()), ("r0", EAST));
    assert!(matches!(kiro().tokens(401, "{}", Some(&previous), Timestamp::now()), Err(ProviderError::Unauthorized(_))));
    let without_arn = kiro().tokens(200, r#"{"accessToken":"a","refreshToken":"r"}"#, None, Timestamp::now());
    assert!(without_arn.is_err(), "no ARN");
}

#[test]
fn the_profile_arn_picks_the_region_of_both_services() {
    assert_eq!(signed_in(ARN).base(kiro()), "https://runtime.eu-central-1.kiro.dev");
    assert_eq!(kiro().catalog(&signed_in(ARN), "").url, "https://management.eu-central-1.kiro.dev/");
    assert_eq!(signed_in("").base(kiro()), "https://runtime.us-east-1.kiro.dev");
    assert_eq!(signed_in("arn:aws:codewhisperer:evil.host/x:1:p").base(kiro()), "https://runtime.us-east-1.kiro.dev");
}

#[test]
fn a_turn_names_the_bare_model_and_the_account() {
    let mut body = json!({"conversationState": {
        "currentMessage": {"userInputMessage": {"modelId": "kiro-subscription:claude-sonnet-4.5"}},
        "history": [{"userInputMessage": {"modelId": "kiro-subscription:auto"}}, {"assistantResponseMessage": {}}]
    }});
    crate::request::shape(kiro(), None, &mut body).unwrap_or_else(|e| panic!("{e}"));
    kiro().bind(&signed_in(ARN), &mut body);
    assert_eq!(body["conversationState"]["currentMessage"]["userInputMessage"]["modelId"], "claude-sonnet-4.5");
    assert_eq!(body["conversationState"]["history"][0]["userInputMessage"]["modelId"], "auto");
    assert_eq!(body["profileArn"], ARN);
    assert_eq!(body["conversationState"]["currentMessage"]["userInputMessage"]["origin"], "KIRO_CLI");
}

#[test]
fn a_turn_asks_a_thinking_model_for_its_effort_beside_the_conversation() {
    let turn = |model: &ModelOption, effort: &str| {
        let mut body = json!({
            "conversationState": {"currentMessage": {"userInputMessage": {"modelId": model.id}}},
            "output_config": {"effort": effort},
        });
        crate::request::shape(kiro(), Some(model), &mut body).unwrap_or_else(|e| panic!("{e}"));
        body
    };
    let body = turn(&model("claude-opus-4.6"), "xhigh");
    assert_eq!(body["additionalModelRequestFields"], json!({"output_config": {"effort": "high"}}));
    assert!(body.get("output_config").is_none());
    let plain = turn(&model("claude-sonnet-4.5"), "high");
    assert!(plain.get("additionalModelRequestFields").is_none() && plain.get("output_config").is_none());
}

#[test]
fn it_is_served_from_the_profiles_region_with_its_token() {
    let config = signed_in(ARN);
    assert!(config.validate(kiro()).is_ok());
    assert_eq!(config.base(kiro()), "https://runtime.eu-central-1.kiro.dev");
    let headers = config.headers(kiro());
    assert!(headers.contains(&("authorization".into(), "Bearer a".into())));
    assert!(headers.contains(&("x-amz-target".into(), CHAT.into())));
    let agent = header(&headers, "user-agent");
    assert_eq!(agent, Some(setting("user_agent_streaming")));
    assert_eq!(header(&kiro().catalog(&config, "").headers, "user-agent"), Some(setting("user_agent_runtime")));
    assert!(headers.iter().any(|(name, value)| name == "x-amz-user-agent" && Some(value.as_str()) == agent));
    let without_arn = signed_in("");
    assert!(without_arn.validate(kiro()).is_err());
}

#[test]
fn a_page_lists_ids_names_and_limits() {
    let page = kiro().page(&json!({"defaultModel": {"modelId": "auto"}, "models": [
        {"modelId": "auto", "modelName": "auto", "tokenLimits": {"maxInputTokens": 1000000, "maxOutputTokens": 64000}},
        {"modelId": "glm-5", "modelName": "glm-5", "tokenLimits": {"maxInputTokens": 200000}},
        {"modelName": "nameless"}
    ], "nextToken": "n2"}));
    let page = page.unwrap_or_else(|e| panic!("{e}"));
    let ids: Vec<_> = page.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["kiro-subscription:auto", "kiro-subscription:glm-5"]);
    assert_eq!(page.models[0].window, Some(1_000_000));
    assert_eq!(page.models[0].capabilities.as_ref().and_then(|c| c.max_output_tokens), Some(64_000));
    assert_eq!(page.next.as_deref(), Some("n2"));
}

#[test]
fn the_catalog_offers_the_efforts_each_model_accepts() {
    let models = listed(&json!({"models": [
        {"modelId": "claude-opus-4.6", "additionalModelRequestFieldsSchema": schema("output_config", &["low", "medium", "high", "max"])},
        {"modelId": "claude-opus-4.8"},
        {"modelId": "claude-sonnet-4.5"},
        {"modelId": "claude-haiku-4.5"},
        {"modelId": "auto"}
    ]}));
    let efforts: Vec<Vec<&str>> = models.iter().map(crate::efforts).collect();
    assert_eq!(efforts[0], ["low", "medium", "high", "max"]);
    assert_eq!(efforts[1], EFFORTS);
    assert!(efforts[2..].iter().all(Vec::is_empty), "{efforts:?}");
}

#[test]
fn the_catalog_is_asked_of_the_profiles_management_service() {
    let config = Config {
        auth: Some(Tokens {
            refresh_token: "r".into(),
            account_id: "arn:aws:codewhisperer:us-east-1:1:profile/P".into(),
            ..Tokens::bearer("tok")
        }),
        ..Config::default()
    };
    let call = kiro().catalog(&config, "");
    assert_eq!(call.url, "https://management.us-east-1.kiro.dev/");
    assert!(call.headers.contains(&("x-amz-target".into(), MODELS.into())));
    assert!(call.headers.contains(&("authorization".into(), "Bearer tok".into())));
}

#[test]
fn the_request_names_the_cli_the_account_and_the_page() {
    let body = |after: &str| {
        let call = kiro().catalog(&signed_in("arn:p"), after);
        serde_json::from_str::<Value>(call.body.as_deref().unwrap_or_default()).unwrap_or_default()
    };
    assert_eq!(body(""), json!({"origin": "KIRO_CLI", "profileArn": "arn:p"}));
    assert_eq!(body("n2")["nextToken"], "n2");
    assert!(kiro().page(&json!({})).is_err());
}

#[test]
fn each_effort_goes_to_the_output_config_of_a_claude_model() {
    for effort in EFFORTS {
        assert_eq!(sent(&model("claude-opus-4.8"), effort), json!({"output_config": {"effort": effort}}), "{effort}");
    }
}

#[test]
fn an_effort_the_model_lacks_falls_to_the_next_one_below() {
    let sonnet = model("claude-sonnet-4.6");
    assert_eq!(sent(&sonnet, "xhigh"), json!({"output_config": {"effort": "high"}}));
    assert_eq!(sent(&sonnet, "max"), json!({"output_config": {"effort": "max"}}));
}

#[test]
fn the_schema_of_the_catalog_names_the_field_and_the_levels() {
    let opus = json!({"properties": {
        "output_config": {"properties": {"effort": {"enum": ["low", "high"]}}},
        "thinking": {"properties": {"type": {"enum": ["adaptive", "disabled"]}}}
    }});
    let models = listed(&json!({"models": [
        {"modelId": "gpt-5.6-sol", "additionalModelRequestFieldsSchema": schema("reasoning", &["none", "low", "medium", "high", "xhigh", "max"])},
        {"modelId": "claude-opus-5", "additionalModelRequestFieldsSchema": opus},
        {"modelId": "glm-5", "additionalModelRequestFieldsSchema": {"properties": {}}}
    ]}));
    assert_eq!(sent(&models[0], "low"), json!({"reasoning": {"effort": "low"}}));
    assert_eq!(sent(&models[1], "medium"), json!({"output_config": {"effort": "low"}}));
    assert!(models[2].capabilities.as_ref().is_some_and(|caps| !caps.reasoning && caps.reasoning_levels.is_empty()));
}

#[test]
fn models_without_effort_controls_get_no_extra_fields() {
    for id in ["claude-sonnet-4.5", "claude-haiku-4.5", "auto", "glm-5", "claude-opus-4.1"] {
        assert_eq!(sent(&model(id), "high"), Value::Null, "{id}");
    }
    let mut body = json!({"output_config": {"effort": "high"}});
    kiro().shape(None, &mut body, "high").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(body, json!({}));
}

#[test]
fn the_known_claude_families_read_their_version() {
    let ids = ["claude-opus-4.6", "claude-opus-5", "claude-sonnet-5.5", "claude-sonnet-4.5", "claude-haiku-5"];
    let models = listed(&json!({"models": ids.map(|id| json!({"modelId": id}))}));
    let levels: Vec<Vec<String>> =
        models.into_iter().map(|m| m.capabilities.map(|caps| caps.reasoning_levels).unwrap_or_default()).collect();
    assert_eq!(levels[0], ["low", "medium", "high", "max"]);
    assert_eq!(levels[1], EFFORTS);
    assert_eq!(levels[2], EFFORTS);
    assert!(levels[3].is_empty() && levels[4].is_empty());
}

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
    let usage = kiro().windows(&value, Language::Portuguese).unwrap_or_else(|e| panic!("{e}"));
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
    let usage = kiro().windows(&value, Language::Portuguese).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(usage.plan.as_deref(), Some("Q DEVELOPER STANDALONE FREE"));
    assert_eq!(usage.windows.len(), 1);
    assert_eq!(usage.windows[0].label, "Requests · 30 de 50");
    assert!((usage.windows[0].used_percent - 60.0).abs() < f64::EPSILON);
}

#[test]
fn an_answer_without_a_breakdown_is_an_error() {
    assert!(kiro().windows(&json!({"message": "nope"}), Language::Portuguese).is_err());
}

#[test]
fn the_call_goes_to_the_profile_region_with_the_usage_target() {
    let config: Config = serde_json::from_value(json!({
        "auth": {"access_token": "tok", "refresh_token": "r", "account_id": "arn:aws:codewhisperer:eu-central-1:1:profile/X"}
    }))
    .unwrap_or_else(|e| panic!("{e}"));
    let call = kiro().limits(&config).unwrap_or_else(|| panic!("limits"));
    assert_eq!(call.url, "https://q.eu-central-1.amazonaws.com/");
    assert!(call.headers.iter().any(|(name, value)| name == "x-amz-target" && value == USAGE));
    let body: Value = serde_json::from_str(call.body.as_deref().unwrap_or_default()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(body["profileArn"], "arn:aws:codewhisperer:eu-central-1:1:profile/X");
}
