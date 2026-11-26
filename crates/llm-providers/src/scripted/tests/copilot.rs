use domain::model::ModelCapabilities;
use domain::{Language, ModelOption, ModelProvider};
use serde_json::{Value, json};

use super::provider;
use crate::testing::{Log, ScriptedHost};
use crate::{Config, Provider, ProviderError, Tokens};

const CLIENT: &str = "Ov23ctDVkRmgkPke0Mmm";
const DEVICE: &str = "https://github.com/login/device/code";
const TOKEN: &str = "https://github.com/login/oauth/access_token";
const USER: &str = "https://api.github.com/copilot_internal/user";

fn copilot() -> &'static dyn Provider {
    provider("copilot-subscription")
}

fn served(endpoints: &[&str], caps: ModelCapabilities) -> ModelOption {
    let mut model =
        ModelOption::new("copilot-subscription:m", "M").served_by(ModelProvider::new("copilot-subscription"));
    model.capabilities =
        Some(ModelCapabilities { endpoints: endpoints.iter().map(|e| (*e).to_owned()).collect(), ..caps });
    model
}

fn header<'a>(headers: &'a [(String, String)], key: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn listed(page: &Value) -> Result<Vec<ModelOption>, ProviderError> {
    crate::catalog::collect(copilot(), |_| Ok::<_, ProviderError>(page.clone()))
}

fn code(interval: u64, expires_in: u64) -> (u16, String) {
    let body = json!({"device_code": "dc", "user_code": "AB12-CD34", "verification_uri": "https://github.com/login/device",
        "interval": interval, "expires_in": expires_in});
    (200, body.to_string())
}

fn answer(body: Value) -> (u16, String) {
    (200, body.to_string())
}

fn signed(replies: Vec<(u16, String)>) -> (Result<Config, ProviderError>, Log) {
    let host = ScriptedHost::new(replies, []);
    let config = copilot().sign_in(&host);
    let log = std::mem::take(&mut *host.log.borrow_mut());
    (config, log)
}

fn granted() -> Vec<(u16, String)> {
    vec![code(5, 900), answer(json!({"access_token": "gho_1", "token_type": "bearer"})), answer(json!({"login": "u"}))]
}

fn shaped_messages(caps: ModelCapabilities, effort: &str) -> Value {
    let mut body = json!({"max_tokens": 64000, "thinking": {"type": "adaptive", "display": "summarized"},
        "output_config": {"effort": "low"}});
    copilot().shape(Some(&served(&["/v1/messages"], caps)), &mut body, effort).unwrap_or_else(|e| panic!("{e}"));
    body
}

#[test]
fn claude_with_reasoning_effort_thinks_adaptively_at_an_offered_effort() {
    let levels = ["low", "medium", "high"].map(str::to_owned).to_vec();
    let body = shaped_messages(
        ModelCapabilities { reasoning_levels: levels, max_output_tokens: Some(32000), ..ModelCapabilities::default() },
        "max",
    );
    assert_eq!(
        body,
        json!({"max_tokens": 32000, "thinking": {"type": "adaptive", "display": "summarized"},
            "output_config": {"effort": "high"}})
    );
    let adaptive =
        shaped_messages(ModelCapabilities { adaptive_thinking: true, ..ModelCapabilities::default() }, "xhigh");
    assert_eq!(adaptive["output_config"], json!({"effort": "xhigh"}));
    assert_eq!(adaptive["max_tokens"], 8192);
}

#[test]
fn claude_with_thinking_only_gets_the_copilot_cli_budget() {
    let thinking = ModelCapabilities { reasoning: true, ..ModelCapabilities::default() };
    let medium = shaped_messages(thinking.clone(), "medium");
    assert_eq!(medium["thinking"], json!({"type": "enabled", "budget_tokens": 2048}));
    assert!(medium.get("output_config").is_none());
    assert_eq!(shaped_messages(thinking.clone(), "low")["thinking"]["budget_tokens"], 1024);
    assert_eq!(shaped_messages(thinking, "max")["thinking"]["budget_tokens"], 8191, "within max_tokens");
    let plain = shaped_messages(ModelCapabilities::default(), "high");
    assert_eq!(plain, json!({"max_tokens": 8192}));
}

#[test]
fn responses_reason_at_the_nearest_offered_level() {
    let levels = vec!["low".to_owned(), "medium".to_owned(), "high".to_owned()];
    let model = served(&["/responses"], ModelCapabilities { reasoning_levels: levels, ..ModelCapabilities::default() });
    let mut body = json!({"reasoning": {"effort": "high"}});
    copilot().shape(Some(&model), &mut body, "max").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(body["reasoning"], json!({"effort": "high", "summary": "auto"}));
    let plain = served(&["/responses"], ModelCapabilities::default());
    let mut body = json!({"reasoning": {}, "include": ["reasoning.encrypted_content"]});
    copilot().shape(Some(&plain), &mut body, "high").unwrap_or_else(|e| panic!("{e}"));
    assert!(body.get("reasoning").is_none() && body.get("include").is_none());
}

#[test]
fn chat_stays_within_the_model_output_limit() {
    let model = served(&[], ModelCapabilities { max_output_tokens: Some(4096), ..ModelCapabilities::default() });
    let mut body = json!({"max_tokens": 64000});
    copilot().shape(Some(&model), &mut body, "medium").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(body["max_tokens"], 4096);
}

#[test]
fn the_device_code_is_asked_for_with_the_copilot_cli_client() {
    let (config, log) = signed(granted());
    assert!(config.is_ok(), "{config:?}");
    assert_eq!(log.sent[0], (DEVICE.to_owned(), format!("client_id={CLIENT}&scope=read%3Auser")));
    assert_eq!(log.sent[1].0, TOKEN);
    assert!(log.sent[1].1.ends_with("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"));
}

#[test]
fn a_device_answer_gives_the_code_to_type() {
    let (config, log) = signed(granted());
    assert!(config.is_ok(), "{config:?}");
    assert_eq!(log.opened, [("https://github.com/login/device".to_owned(), Some("AB12-CD34".to_owned()))]);
    assert_eq!(log.waits, [std::time::Duration::from_secs(6)]);
    let (refused, _) = signed(vec![(400, code(5, 899).1)]);
    assert!(refused.is_err());
    let (empty, _) = signed(vec![answer(json!({}))]);
    assert!(empty.is_err());
}

#[test]
fn polling_waits_slows_down_or_gets_the_token() {
    let mut replies = vec![code(5, 900), answer(json!({"error": "authorization_pending"}))];
    replies.extend(granted().into_iter().skip(1));
    let (config, log) = signed(replies);
    assert_eq!(config.map(|config| config.access_token().to_owned()), Ok("gho_1".into()));
    assert_eq!(log.sent.len(), 4);
    let denied = answer(json!({"error": "access_denied", "error_description": "The user said no"}));
    let (refused, _) = signed(vec![code(5, 900), denied]);
    assert_eq!(refused.map(|_| ()), Err(ProviderError::Unauthorized("The user said no".into())));
    let (failed, _) = signed(vec![code(5, 900), (500, String::new())]);
    assert!(failed.is_err());
}

#[test]
fn pacing_follows_the_copilot_cli() {
    let replies = vec![
        code(5, 900),
        answer(json!({"error": "slow_down", "interval": 10})),
        answer(json!({"error": "slow_down"})),
        answer(json!({"error": "slow_down", "interval": 58})),
        answer(json!({"access_token": "gho_1"})),
        answer(json!({"login": "u"})),
    ];
    let (config, log) = signed(replies);
    assert!(config.is_ok(), "{config:?}");
    assert_eq!(log.waits.iter().map(std::time::Duration::as_secs).collect::<Vec<_>>(), [6, 15, 15, 60]);
}

#[test]
fn the_api_base_comes_from_the_copilot_user() {
    let base = |user: (u16, String)| {
        let mut replies = granted();
        replies[2] = user;
        signed(replies).0.map(|config| config.base(copilot()))
    };
    let business = answer(json!({"login": "u", "endpoints": {"api": "https://api.business.githubcopilot.com/"}}));
    assert_eq!(base(business), Ok("https://api.business.githubcopilot.com".into()));
    assert_eq!(base(answer(json!({"login": "u"}))), Ok("https://api.githubcopilot.com".into()));
    assert_eq!(base(answer(json!({"endpoints": {"api": "http://evil"}}))), Ok("https://api.githubcopilot.com".into()));
    assert!(matches!(base((404, String::new())), Err(ProviderError::Unauthorized(_))));
}

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
    let usage = copilot().windows(&value, Language::Portuguese).unwrap_or_else(|e| panic!("{e}"));
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
    let usage = copilot().windows(&value, Language::Portuguese).unwrap_or_else(|e| panic!("{e}"));
    assert!(usage.windows.is_empty() && usage.plan.is_none());
    assert!(copilot().windows(&json!("x"), Language::Portuguese).is_err());
}

#[test]
fn the_call_goes_to_the_github_user_with_the_github_token() {
    let call = copilot().limits(&Config { auth: Some(Tokens::bearer("gho_x")), ..Config::default() });
    let call = call.unwrap_or_else(|| panic!("limits"));
    assert_eq!(call.url, USER);
    assert!(call.headers.contains(&("authorization".to_owned(), "Bearer gho_x".to_owned())));
}

#[test]
fn each_model_is_served_on_the_best_api_it_offers() {
    let all = ["/chat/completions", "/responses", "/v1/messages"];
    assert_eq!(copilot().wire(Some(&served(&all, ModelCapabilities::default()))), "messages");
    assert_eq!(copilot().wire(Some(&served(&all[..2], ModelCapabilities::default()))), "responses");
    assert_eq!(copilot().wire(Some(&served(&all[..1], ModelCapabilities::default()))), "chat");
    assert_eq!(copilot().wire(None), "chat");
    assert_eq!(copilot().path(Some(&served(&all, ModelCapabilities::default()))), "/v1/messages");
}

#[test]
fn a_bare_github_token_is_taken_but_not_a_classic_one() {
    let config = Config::parse(copilot(), " gho_abc ").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(config.access_token(), "gho_abc");
    assert!(!config.needs_refresh(copilot(), true), "GitHub tokens are not refreshed");
    assert!(Config::parse(copilot(), "ghp_classic").is_err());
    assert!(Config::parse(copilot(), "").is_err());
    assert!(config.headers(copilot()).contains(&("authorization".into(), "Bearer gho_abc".into())));
    assert_eq!(config.base(copilot()), "https://api.githubcopilot.com");
}

#[test]
fn the_catalog_lists_its_data_page_once() {
    let page =
        json!({"data": [{"id": "claude-sonnet-4.5", "name": "Claude Sonnet 4.5"}, {"id": "x", "custom_model": true}]});
    assert_eq!(
        listed(&page).map(|m| m.into_iter().map(|m| m.id).collect::<Vec<_>>()),
        Ok(vec!["copilot-subscription:claude-sonnet-4.5".to_owned()])
    );
}

#[test]
fn calls_carry_the_copilot_cli_identity() {
    let sent = Config { auth: Some(Tokens::bearer("gho_x")), ..Config::default() }.headers(copilot());
    assert_eq!(header(&sent, "authorization"), Some("Bearer gho_x"));
    assert_eq!(header(&sent, "copilot-integration-id"), Some("copilot-developer-cli"));
    assert_eq!(header(&sent, "openai-intent"), Some("conversation-agent"));
    assert_eq!(header(&sent, "x-github-api-version"), Some("2026-06-01"));
    assert_eq!(header(&sent, "editor-version"), Some("copilot/1.0.62"));
}

#[test]
fn only_a_turn_the_user_started_is_billed_as_theirs() {
    let messages = served(&["/v1/messages"], ModelCapabilities::default());
    let responses = served(&["/responses"], ModelCapabilities::default());
    let turn = |model: Option<&ModelOption>, body: &Value| copilot().turn_headers(model, body);
    let user = json!({"messages": [{"role": "assistant", "content": "hi"}, {"role": "user", "content": "go"}]});
    assert_eq!(header(&turn(None, &user), "x-initiator"), Some("user"));
    let tool = json!({"messages": [{"role": "assistant"}, {"role": "tool", "content": "ok"}, {"role": "user", "content": "note"}]});
    assert_eq!(header(&turn(None, &tool), "x-initiator"), Some("agent"));
    let anthropic = json!({"messages": [{"role": "user", "content": [{"type": "tool_result"}]}]});
    assert_eq!(header(&turn(Some(&messages), &anthropic), "x-interaction-type"), Some("conversation-agent"));
    let called = json!({"input": [{"type": "function_call"}, {"type": "function_call_output"}]});
    assert_eq!(header(&turn(Some(&responses), &called), "x-initiator"), Some("agent"));
    let fresh = json!({"input": [{"type": "message", "role": "user", "content": [{"type": "input_image"}]}]});
    let sent = turn(Some(&responses), &fresh);
    assert_eq!((header(&sent, "x-initiator"), header(&sent, "copilot-vision-request")), (Some("user"), Some("true")));
}

#[test]
fn an_entry_gives_its_window_limits_effort_and_apis() {
    let entry = json!({"id": "gpt-5", "name": "GPT-5", "model_picker_enabled": true,
        "capabilities": {"type": "chat", "limits": {"max_context_window_tokens": 400000, "max_output_tokens": 128000},
            "supports": {"tool_calls": true, "reasoning_effort": ["low", "medium", "high"]}},
        "supported_endpoints": ["/chat/completions", "/responses"]});
    let found = listed(&json!({"data": [entry]})).unwrap_or_default().pop().unwrap_or_else(|| panic!("listed"));
    assert_eq!((found.id.as_str(), found.label.as_str()), ("copilot-subscription:gpt-5", "GPT-5"));
    assert_eq!(found.window, Some(400000));
    let caps = found.capabilities.unwrap_or_default();
    assert!(caps.reasoning);
    assert_eq!((caps.max_output_tokens, caps.reasoning_levels.len()), (Some(128000), 3));
    assert_eq!(caps.endpoints, ["/chat/completions", "/responses"]);
}

#[test]
fn the_picker_hides_custom_disabled_and_non_chat_models() {
    let hidden = json!({"data": [
        {"id": "c", "custom_model": true},
        {"id": "d", "model_picker_enabled": false, "policy": {"state": "unconfigured"}},
        {"id": "e", "capabilities": {"type": "embeddings"}},
        {"id": ""},
    ]});
    assert_eq!(listed(&hidden).map(|models| models.len()), Ok(0));
    let enabled = json!({"data": [{"id": "o", "model_picker_enabled": false, "policy": {"state": "enabled"}}]});
    assert_eq!(
        listed(&enabled).map(|models| models.into_iter().map(|m| m.label).collect::<Vec<_>>()),
        Ok(vec!["o".into()])
    );
}

#[test]
fn the_device_flow_waits_for_approval_and_finds_the_account_api() {
    let replies = vec![
        code(5, 900),
        answer(json!({"error": "authorization_pending"})),
        answer(json!({"error": "slow_down", "interval": 10})),
        answer(json!({"access_token": "gho_new", "token_type": "bearer"})),
        answer(json!({"login": "u", "endpoints": {"api": "https://api.individual.githubcopilot.com"}})),
    ];
    let (config, log) = signed(replies);
    let config = config.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(log.opened, [("https://github.com/login/device".to_owned(), Some("AB12-CD34".to_owned()))]);
    assert_eq!(log.waits.iter().map(std::time::Duration::as_secs).collect::<Vec<_>>(), [6, 6, 15]);
    assert_eq!(config.access_token(), "gho_new");
    assert_eq!(config.base(copilot()), "https://api.individual.githubcopilot.com");
    assert_eq!(log.sent[0].1, format!("client_id={CLIENT}&scope=read%3Auser"));
    assert_eq!(log.sent[4].0, USER);
}

#[test]
fn a_refused_or_expired_code_ends_the_sign_in() {
    let denied = answer(json!({"error": "access_denied", "error_description": "denied"}));
    let (error, _) = signed(vec![code(5, 10), denied]);
    assert!(error.map(|_| ()).err().unwrap_or_else(|| panic!("denied")).to_string().contains("denied"));
    let pending = answer(json!({"error": "authorization_pending"}));
    let (error, _) = signed(vec![code(5, 10), pending.clone(), pending]);
    assert!(error.map(|_| ()).err().unwrap_or_else(|| panic!("expired")).to_string().contains("expired"));
}
