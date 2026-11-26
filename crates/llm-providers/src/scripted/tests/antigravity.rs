use jiff::{SignedDuration, Timestamp};
use serde_json::{Value, json};

use super::provider;
use crate::testing::{Log, ScriptedHost};
use crate::{Config, Provider, ProviderError, Tokens};

const CLIENT: &str = "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
const SECRET: &str = "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";
const AUTHORIZE: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const USERINFO: &str = "https://www.googleapis.com/oauth2/v1/userinfo?alt=json";
const LOAD: &str = "https://daily-cloudcode-pa.googleapis.com/v1internal:loadCodeAssist";
const ONBOARD: &str = "https://daily-cloudcode-pa.googleapis.com/v1internal:onboardUser";
const DEFAULT_PROJECT: &str = "rising-fact-p41fc";
const FALLBACK: usize = 7;
const TOKENS: &str = r#"{"access_token":"a","refresh_token":"r","expires_in":3599}"#;

fn antigravity() -> &'static dyn Provider {
    provider("antigravity-subscription")
}

fn signed_in() -> Config {
    let tokens = Tokens { refresh_token: "r".into(), account_id: "proj-1".into(), ..Tokens::bearer("a") };
    Config { auth: Some(tokens), ..Config::default() }
}

fn reply(body: &str) -> (u16, String) {
    (200, body.to_owned())
}

fn signed(replies: Vec<(u16, String)>) -> (Result<Tokens, ProviderError>, Log) {
    let host = ScriptedHost::new(replies, [("/oauth-callback", "code=c&state=r2x32&scope=x")]);
    let tokens = antigravity().sign_in(&host).and_then(|config| config.refreshable().cloned());
    let log = std::mem::take(&mut *host.log.borrow_mut());
    (tokens, log)
}

fn onboarded_with(loaded: Value) -> Log {
    let done = json!({"done": true, "response": {"cloudaicompanionProject": {"id": "p"}}});
    let (tokens, log) = signed(vec![reply(TOKENS), reply(&loaded.to_string()), reply(&done.to_string())]);
    assert_eq!(tokens.map(|tokens| tokens.account_id), Ok("p".into()));
    log
}

fn tier(log: &Log) -> Value {
    assert_eq!(log.sent[2].0, ONBOARD);
    let body: Value = serde_json::from_str(&log.sent[2].1).unwrap_or_default();
    body["tier_id"].clone()
}

fn page(value: &Value) -> Vec<domain::ModelOption> {
    antigravity().page(value).unwrap_or_else(|e| panic!("{e}")).models
}

#[test]
fn the_browser_asks_google_for_offline_access_with_pkce() {
    let (tokens, log) = signed(vec![reply(TOKENS), reply(r#"{"cloudaicompanionProject":"proj-1"}"#)]);
    assert!(tokens.is_ok(), "{tokens:?}");
    let url = &log.opened[0].0;
    assert!(url.starts_with(&format!("{AUTHORIZE}?client_id={CLIENT}")), "{url}");
    let challenge = crate::oauth::challenge("r1x64");
    for wanted in [
        "redirect_uri=http%3A%2F%2Flocalhost%3A51121%2Foauth-callback".to_owned(),
        "scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fcloud-platform+".to_owned(),
        format!("code_challenge={challenge}&code_challenge_method=S256&state=r2x32&access_type=offline&prompt=consent"),
    ] {
        assert!(url.contains(&wanted), "{wanted} in {url}");
    }
}

#[test]
fn the_code_and_the_refresh_token_go_to_google_as_a_form() {
    let (tokens, log) = signed(vec![reply(TOKENS), reply(r#"{"cloudaicompanionProject":"proj-1"}"#)]);
    assert!(tokens.is_ok(), "{tokens:?}");
    let (url, body) = &log.sent[0];
    assert_eq!(url, TOKEN);
    assert!(body.contains("grant_type=authorization_code") && body.contains("code_verifier=r1x64"), "{body}");
    let previous = Tokens { refresh_token: "r0".into(), ..Tokens::bearer("a") };
    let refreshed = antigravity().refresh(&previous).unwrap_or_else(|| panic!("refreshable"));
    assert_eq!((refreshed.url.as_str(), refreshed.content_type), (TOKEN, "application/x-www-form-urlencoded"));
    assert!(refreshed.body.contains("grant_type=refresh_token&refresh_token=r0"), "{}", refreshed.body);
    assert!(refreshed.body.contains(&format!("client_secret={SECRET}")));
}

#[test]
fn a_refresh_keeps_the_project_the_refresh_token_and_the_email() {
    let previous = Tokens {
        refresh_token: "r0".into(),
        account_id: "proj-1".into(),
        email: Some("me@example.com".into()),
        ..Tokens::bearer("old")
    };
    let now = Timestamp::now();
    let tokens = |status: u16, body: &str, previous: Option<&Tokens>| antigravity().tokens(status, body, previous, now);
    let fresh =
        tokens(200, r#"{"access_token":"new","expires_in":3599}"#, Some(&previous)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((fresh.access_token.as_str(), fresh.refresh_token.as_str()), ("new", "r0"));
    assert_eq!(fresh.account_id, "proj-1");
    assert_eq!(fresh.email.as_deref(), Some("me@example.com"));
    assert_eq!(fresh.expires_at, Some(now + SignedDuration::from_secs(3599)));
    assert!(tokens(400, r#"{"error":"invalid_grant"}"#, Some(&previous)).is_err());
    assert!(tokens(200, r#"{"access_token":"a"}"#, None).is_err(), "no refresh token");
}

#[test]
fn the_project_is_read_as_a_name_or_an_object() {
    let project = |loaded: &str| signed(vec![reply(TOKENS), reply(loaded)]).0.map(|tokens| tokens.account_id);
    assert_eq!(project(r#"{"cloudaicompanionProject":"p-1"}"#), Ok("p-1".into()));
    assert_eq!(project(r#"{"cloudaicompanionProject":{"id":"p-2"}}"#), Ok("p-2".into()));
    let (_, log) = signed(vec![reply(TOKENS), reply(r#"{"cloudaicompanionProject":""}"#)]);
    assert_eq!(log.sent[2].0, ONBOARD, "an empty project is no project");
    let (_, log) = signed(vec![reply(TOKENS), reply("{}")]);
    assert_eq!(log.sent[2].0, ONBOARD);
}

#[test]
fn onboarding_takes_the_default_tier_and_waits_until_done() {
    let tiers = json!({"allowedTiers": [{"id": "legacy-tier"}, {"id": "standard-tier", "isDefault": true}]});
    assert_eq!(tier(&onboarded_with(tiers)), "standard-tier");
    assert_eq!(tier(&onboarded_with(json!({"allowedTiers": [{"id": "legacy-tier"}]}))), "legacy-tier");
    assert_eq!(tier(&onboarded_with(json!({"currentTier": {"id": "g1-pro-tier"}}))), "g1-pro-tier");
    let log = onboarded_with(json!({}));
    assert_eq!(tier(&log), "free-tier");
    let body: Value = serde_json::from_str(&log.sent[2].1).unwrap_or_default();
    assert_eq!(body["metadata"]["ide_type"], "ANTIGRAVITY");
    assert!(log.waits.is_empty());
}

#[test]
fn the_email_comes_from_the_user_info() {
    let email = |info: &str| {
        let replies = vec![reply(TOKENS), reply(r#"{"cloudaicompanionProject":"p"}"#), reply(info)];
        signed(replies).0.map(|tokens| tokens.email)
    };
    assert_eq!(email(r#"{"email":"me@example.com","id":"1"}"#), Ok(Some("me@example.com".into())));
    assert_eq!(email(r#"{"id":"1"}"#), Ok(None));
}

#[test]
fn a_turn_goes_to_the_daily_host_for_the_accounts_project() {
    let config = signed_in();
    assert!(config.validate(antigravity()).is_ok());
    assert_eq!(
        format!("{}{}", config.base(antigravity()), antigravity().path(None)),
        "https://daily-cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse"
    );
    let headers = config.headers(antigravity());
    assert!(headers.contains(&("authorization".into(), "Bearer a".into())));
    assert!(headers.iter().any(|(name, value)| name == "user-agent" && value.starts_with("antigravity/hub/")));
    let mut body = json!({"model": "antigravity-subscription:gemini-3.8-flash-high", "project": ""});
    crate::request::shape(antigravity(), None, &mut body).unwrap_or_else(|e| panic!("{e}"));
    antigravity().bind(&config, &mut body);
    assert_eq!((body["model"].as_str(), body["project"].as_str()), (Some("gemini-3.8-flash-high"), Some("proj-1")));
}

#[test]
fn a_credential_without_a_project_asks_for_a_new_sign_in() {
    let without =
        Config { auth: Some(Tokens { refresh_token: "r".into(), ..Tokens::bearer("a") }), ..Config::default() };
    assert!(matches!(without.validate(antigravity()), Err(ProviderError::Unauthorized(_))));
}

#[test]
fn an_empty_catalog_falls_back_to_the_known_models() {
    let models =
        crate::catalog::collect(antigravity(), |_| Ok::<_, ProviderError>(json!({"models": {}}))).unwrap_or_default();
    assert_eq!(models.len(), FALLBACK);
    assert!(antigravity().serves("gemini-3.8-flash-high") && antigravity().serves("claude-opus-5-5-high"));
}

#[test]
fn the_catalog_is_asked_for_the_accounts_project() {
    let tokens = Tokens { refresh_token: "r".into(), account_id: "proj-1".into(), ..Tokens::bearer("tok") };
    let call = antigravity().catalog(&Config { auth: Some(tokens), ..Config::default() }, "");
    assert_eq!(call.url, "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels");
    assert_eq!(call.body.as_deref(), Some(r#"{"project":"proj-1"}"#));
    assert!(call.headers.contains(&("authorization".into(), "Bearer tok".into())));
}

#[test]
fn a_map_of_models_lists_ids_names_and_limits_without_the_editors_own() {
    let models = page(&json!({"models": {
        "gemini-3.8-flash-high": {"displayName": "Gemini 3.8 Flash (High)", "maxTokens": 1_048_576, "maxOutputTokens": 65_536},
        "claude-opus-5-5-high": {"displayName": "Claude Opus 5.5"},
        "chat_20706": {},
        "tab_flash_lite_preview": {},
        "gemini-3-pro-image": {},
    }}));
    let ids: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        ["antigravity-subscription:claude-opus-5-5-high", "antigravity-subscription:gemini-3.8-flash-high"]
    );
    assert_eq!(models[1].label, "Gemini 3.8 Flash (High)");
    assert_eq!(models[1].window, Some(1_048_576));
    assert_eq!(models[1].capabilities.as_ref().and_then(|c| c.max_output_tokens), Some(65_536));
}

#[test]
fn a_list_of_models_and_an_unknown_answer_are_read_too() {
    let models = page(&json!({"models": [{"name": "models/gpt-oss-120b-medium"}]}));
    assert_eq!(models[0].id, "antigravity-subscription:gpt-oss-120b-medium");
    assert_eq!(models[0].capabilities.as_ref().map(|c| c.reasoning), Some(false));
    assert!(page(&json!({})).is_empty());
    let fallback = crate::catalog::collect(antigravity(), |_| Ok::<_, ProviderError>(json!({}))).unwrap_or_default();
    assert_eq!(fallback.len(), FALLBACK);
}

#[test]
fn a_sign_in_finds_the_accounts_project_and_email() {
    let replies = vec![
        reply(TOKENS),
        reply(r#"{"cloudaicompanionProject":"proj-1","currentTier":{"id":"free-tier"}}"#),
        reply(r#"{"email":"me@example.com"}"#),
    ];
    let (tokens, log) = signed(replies);
    let tokens = tokens.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((tokens.access_token.as_str(), tokens.account_id.as_str()), ("a", "proj-1"));
    assert_eq!(tokens.email.as_deref(), Some("me@example.com"));
    assert!(log.opened[0].0.starts_with(AUTHORIZE), "{:?}", log.opened);
    let urls: Vec<_> = log.sent.iter().map(|(url, _)| url.as_str()).collect();
    assert_eq!(urls, [TOKEN, LOAD, USERINFO]);
}

#[test]
fn an_account_without_a_project_is_onboarded_until_done() {
    let replies = vec![
        reply(TOKENS),
        reply(r#"{"allowedTiers":[{"id":"free-tier","isDefault":true}]}"#),
        reply(r#"{"done":false}"#),
        reply(r#"{"done":true,"response":{"cloudaicompanionProject":{"id":"proj-2"}}}"#),
        (404, String::new()),
    ];
    let (tokens, log) = signed(replies);
    let tokens = tokens.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(tokens.account_id, "proj-2");
    assert_eq!(tokens.email, None);
    assert!(log.sent[2].1.contains(r#""tier_id":"free-tier""#), "{}", log.sent[2].1);
    assert_eq!(log.waits, [std::time::Duration::from_secs(5)]);
}

#[test]
fn without_any_project_the_shared_default_is_used() {
    let failed = || (500, String::new());
    let (tokens, _) = signed(vec![reply(TOKENS), failed(), failed(), failed()]);
    assert_eq!(tokens.map(|tokens| tokens.account_id), Ok(DEFAULT_PROJECT.to_owned()));
}

#[test]
fn a_refused_code_fails_the_sign_in() {
    let host = ScriptedHost::new(
        [(400, r#"{"error":"invalid_grant"}"#.to_owned())],
        [("/oauth-callback", "code=c&state=r2x32")],
    );
    assert!(matches!(antigravity().sign_in(&host), Err(ProviderError::Unauthorized(_))));
    assert!(host.log.borrow().answered[0].starts_with("failed"));
}
