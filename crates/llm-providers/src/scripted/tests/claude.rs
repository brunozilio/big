use domain::model::ModelCapabilities;
use domain::{Language, ModelOption, ModelProvider};
use serde_json::{Value, json};

use super::provider;
use crate::model::{BlockingTurn, CancellationToken, Intent, ModelError, ToolSpec};
use crate::providers::testing::{client, effort_body, header, sent_turn};
use crate::testing::ScriptedHost;
use crate::{Config, Provider, ProviderError, Tokens};

const CLAUDE: ModelProvider = ModelProvider::new("claude-subscription");
const USAGE: &str = "https://api.anthropic.com/api/oauth/usage";
const MESSAGES: &str = include_str!("../../../tests/fixtures/messages_tool_use.sse");

fn claude() -> &'static dyn Provider {
    provider("claude-subscription")
}

fn shown(usage: &domain::billing::PlanUsage) -> Vec<(&str, &str, f64, Option<String>)> {
    usage.windows.iter().map(|w| (w.id.as_str(), w.label.as_str(), w.used_percent, w.resets_at.clone())).collect()
}

#[test]
fn a_turn_goes_to_the_messages_api_signed_in() {
    let (sent, body) = sent_turn(CLAUDE, MESSAGES);
    assert_eq!(body["model"], "model-1");
    assert_eq!(sent.url, "https://api.anthropic.com/v1/messages");
    assert!(header(&sent, "x-api-key").is_none());
    assert_eq!(header(&sent, "anthropic-beta").as_deref(), Some("claude-code-20250219,oauth-2025-04-20"));
    assert_eq!(header(&sent, "anthropic-client-platform").as_deref(), Some("claude_code_cli"));
    assert_eq!(header(&sent, "anthropic-client-version").as_deref(), Some("2.1.330"));
    assert_eq!(header(&sent, "user-agent").as_deref(), Some("claude-cli/2.1.330 (external, cli)"));
    assert_eq!(body["system"][0]["text"], "You are Claude Code, Anthropic's official CLI for Claude.");
    assert_eq!(body["system"][1]["text"], "Help the user.");
}

fn cached_as(intent: Intent) -> Value {
    let (client, http) = client(CLAUDE, [(200, MESSAGES.to_owned())]);
    let mut request = crate::providers::testing::request("claude-subscription:model-1");
    request.intent = intent;
    request.tools.push(ToolSpec { name: "Read".into(), description: "Reads".into(), schema: json!({}) });
    client.turn_blocking(&request, &mut |_| {}, &CancellationToken::new()).expect("stream");
    serde_json::from_str(&http.sent()[0].body).expect("body")
}

#[test]
fn the_system_prompt_the_tools_and_the_conversation_are_cached_for_the_short_lifetime() {
    let body = cached_as(Intent::default());
    let short = json!({"type": "ephemeral", "ttl": "5m"});
    assert_eq!(body["cache_control"], short);
    assert!(body["system"][0].get("cache_control").is_none());
    assert_eq!(body["system"][1]["cache_control"], short);
    assert_eq!(body["tools"][0]["cache_control"], short);
    assert!(body.get("intent").is_none(), "the intent never reaches the API");
}

#[test]
fn a_session_turn_keeps_its_cache_for_the_configured_long_lifetime() {
    let body = cached_as(Intent::SESSION);
    assert_eq!(body["cache_control"], json!({"type": "ephemeral", "ttl": "1h"}));
    assert_eq!(body["tools"][0]["cache_control"], json!({"type": "ephemeral", "ttl": "1h"}));
}

#[test]
fn a_one_off_call_marks_nothing_to_cache() {
    let body = cached_as(Intent::ONCE);
    assert!(body.get("cache_control").is_none());
    assert!(body["system"][1].get("cache_control").is_none());
    assert!(body["tools"][0].get("cache_control").is_none());
}

#[test]
fn the_chosen_effort_reaches_an_adaptive_model() {
    let body = effort_body(CLAUDE, "claude-opus-5-5", MESSAGES);
    assert_eq!(body["output_config"]["effort"], "high");
    assert_eq!(body["thinking"]["type"], "adaptive");
}

#[test]
fn the_fallback_catalog_needs_a_valid_empty_answer() {
    let models = |status: u16, body: &str| client(CLAUDE, [(status, body.to_owned())]).0.models();
    assert_eq!(models(200, "{\"data\":[]}").expect("fallback").len(), 4);
    assert!(matches!(models(401, "{}"), Err(ModelError::Unauthorized(_))));
    assert!(models(200, "{}").is_err());
}

#[test]
fn each_window_with_a_utilization_is_kept_and_nulls_are_skipped() {
    let value = json!({
        "five_hour": {"utilization": 42.0, "resets_at": "2026-11-23T18:00:00.123+00:00"},
        "seven_day": {"utilization": 10, "resets_at": null},
        "seven_day_opus": null,
        "seven_day_sonnet": {"utilization": null, "resets_at": "2026-11-28T00:00:00+00:00"},
        "extra_usage": {"is_enabled": false}
    });
    let usage = claude().windows(&value, Language::Portuguese).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(usage.plan, None);
    assert_eq!(
        shown(&usage),
        [
            ("five_hour", "Sessão (5 h)", 42.0, Some("2026-11-23T18:00:00.123Z".to_owned())),
            ("seven_day", "Semana", 10.0, None),
        ]
    );
}

#[test]
fn a_body_that_is_not_an_object_is_refused() {
    assert!(claude().windows(&json!([]), Language::Portuguese).is_err());
}

#[test]
fn the_call_asks_for_json_with_the_subscription_token() {
    let tokens = Tokens { refresh_token: "r".into(), ..Tokens::bearer("a") };
    let config = Config { auth: Some(tokens), ..Config::default() };
    let call = claude().limits(&config).unwrap_or_else(|| panic!("limits"));
    assert_eq!(call.url, USAGE);
    assert!(call.headers.contains(&("authorization".to_owned(), "Bearer a".to_owned())));
    assert!(call.headers.contains(&("accept".to_owned(), "application/json".to_owned())));
    assert!(call.headers.iter().any(|(name, value)| name == "anthropic-beta" && value.contains("oauth-2025-04-20")));
}

#[test]
fn the_client_asks_the_usage_once_and_a_provider_without_limits_asks_nothing() {
    let answer = json!({"five_hour": {"utilization": 7, "resets_at": null}}).to_string();
    let (claude, http) = client(CLAUDE, [(200, answer)]);
    let usage =
        claude.limits(Language::Portuguese).unwrap_or_else(|e| panic!("{e}")).unwrap_or_else(|| panic!("supported"));
    assert_eq!(usage.windows.len(), 1);
    let sent = http.sent();
    assert_eq!((sent.len(), sent[0].url.as_str()), (1, USAGE));
    assert_eq!(header(&sent[0], "authorization").as_deref(), Some("Bearer access-1"));
    let (grok, http) = client(ModelProvider::new("grok-subscription"), []);
    assert_eq!(grok.limits(Language::Portuguese).ok(), Some(None));
    assert!(http.sent().is_empty());
    let (failing, _) = client(CLAUDE, [(401, "{}".to_owned())]);
    assert!(failing.limits(Language::Portuguese).is_err());
}

#[test]
fn the_browser_is_sent_with_the_inference_scope_and_the_code_is_exchanged_as_json_with_the_state() {
    let host = ScriptedHost::new(
        [(200, json!({"access_token": "a", "refresh_token": "r", "expires_in": 3600}).to_string())],
        [("/callback", "code=c&state=r2x32")],
    );
    let config = claude().sign_in(&host).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(config.access_token(), "a");
    let log = host.log.borrow();
    let url = &log.opened[0].0;
    assert!(url.starts_with("https://claude.com/cai/oauth/authorize?code=true&client_id="), "{url}");
    assert!(url.contains("scope=user%3Ainference"), "{url}");
    assert_eq!(log.sent[0].0, "https://platform.claude.com/v1/oauth/token");
    let body: Value = serde_json::from_str(&log.sent[0].1).unwrap_or_default();
    assert_eq!(
        (body["state"].as_str(), body["client_id"].as_str(), body["code"].as_str()),
        (Some("r2x32"), Some("9d1c250a-e61b-44d9-88ed-5944d1962f5e"), Some("c"))
    );
}

#[test]
fn it_speaks_as_claude_code() {
    let mut body = json!({"model": "claude-subscription:claude-sonnet-4-5", "system": [{"type": "text", "text": "x"}]});
    crate::request::shape(claude(), None, &mut body).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(body["system"][0]["text"], "You are Claude Code, Anthropic's official CLI for Claude.");
    assert_eq!(body["max_tokens"], 8192);
}

#[test]
fn the_output_limit_is_the_smaller_of_what_the_model_declares_and_what_the_request_asks() {
    let shaped = |declared: Option<u32>, asked: u64| {
        let mut model = ModelOption::new("claude-subscription:claude-sonnet-4-5", "Sonnet");
        model.capabilities = Some(ModelCapabilities { max_output_tokens: declared, ..ModelCapabilities::default() });
        let mut body = json!({"model": "claude-subscription:claude-sonnet-4-5", "max_tokens": asked});
        crate::request::shape(claude(), Some(&model), &mut body).unwrap_or_else(|e| panic!("{e}"));
        body["max_tokens"].as_u64()
    };
    assert_eq!(shaped(Some(32_000), 64_000), Some(32_000), "the model's limit");
    assert_eq!(shaped(Some(128_000), 64_000), Some(64_000), "the request's limit");
    assert_eq!(shaped(None, 64_000), Some(8192), "a model that declares nothing");
}

#[test]
fn an_empty_catalog_falls_back_to_the_current_priced_family() {
    let models = crate::catalog::collect(claude(), |_| Ok::<_, ProviderError>(json!({"data": []}))).unwrap_or_default();
    assert_eq!(models.len(), 4);
    assert!(crate::catalog::collect(claude(), |_| Ok::<_, ProviderError>(json!({}))).is_err());
    for model in &models {
        let id = model.id.rsplit(':').next().unwrap_or_default();
        assert!(crate::rates(claude().vendor(), id, 0).is_some(), "{id}");
    }
}

#[test]
fn it_needs_both_tokens() {
    let config = Config { auth: Some(Tokens::bearer("a")), ..Config::default() };
    assert!(config.validate(claude()).is_err());
    let tokens = Tokens { refresh_token: "r".into(), ..Tokens::bearer("a") };
    assert!(Config { auth: Some(tokens), ..Config::default() }.validate(claude()).is_ok());
}
