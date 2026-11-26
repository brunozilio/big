use jiff::Timestamp;
use serde_json::{Value, json};

use super::provider;
use crate::testing::{Log, ScriptedHost};
use crate::{Config, Provider, ProviderError, Tokens};

const CLIENT: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const CODE: &str = "https://auth.x.ai/oauth2/device/code";
const TOKEN: &str = "https://auth.x.ai/oauth2/token";

fn grok() -> &'static dyn Provider {
    provider("grok-subscription")
}

fn signed_in() -> Config {
    let tokens = Tokens { refresh_token: "r".into(), ..Tokens::bearer("a") };
    Config { auth: Some(tokens), ..Config::default() }
}

fn code(interval: u64, expires_in: u64) -> (u16, String) {
    let body = json!({"device_code": "dc", "user_code": "AB-12", "verification_uri": "https://accounts.x.ai/device",
        "verification_uri_complete": "https://accounts.x.ai/device?code=AB-12", "interval": interval, "expires_in": expires_in});
    (200, body.to_string())
}

fn polled(status: u16, body: Value) -> (u16, String) {
    (status, body.to_string())
}

fn granted() -> (u16, String) {
    polled(200, json!({"access_token": "at", "refresh_token": "rt", "expires_in": 21600}))
}

fn signed(replies: Vec<(u16, String)>) -> (Result<Config, ProviderError>, Log) {
    let host = ScriptedHost::new(replies, []);
    let config = grok().sign_in(&host);
    let log = std::mem::take(&mut *host.log.borrow_mut());
    (config, log)
}

fn seconds(log: &Log) -> Vec<u64> {
    log.waits.iter().map(std::time::Duration::as_secs).collect()
}

#[test]
fn a_subscription_needs_both_tokens() {
    assert!(signed_in().validate(grok()).is_ok());
    let access_only = Config { auth: Some(Tokens::bearer("a")), ..Config::default() };
    assert!(access_only.validate(grok()).is_err());
    assert!(Config::parse(grok(), "xai-key").is_err());
}

#[test]
fn turns_go_to_the_xai_responses_api_with_the_bearer() {
    let config = signed_in();
    assert_eq!(config.base(grok()), "https://api.x.ai/v1");
    assert!(config.headers(grok()).contains(&("authorization".into(), "Bearer a".into())));
    assert_eq!(grok().path(None), "/responses");
}

#[test]
fn a_refresh_is_a_form_to_the_xai_token_endpoint() {
    let request = grok().refresh(&Tokens { refresh_token: "r0".into(), ..Tokens::bearer("a") });
    let request = request.unwrap_or_else(|| panic!("refreshable"));
    assert_eq!(request.url, TOKEN);
    assert!(request.body.contains("grant_type=refresh_token") && request.body.contains("refresh_token=r0"));
    assert!(request.body.contains(&format!("client_id={CLIENT}")));
}

#[test]
fn a_rotated_refresh_token_replaces_the_old_one() {
    let previous = Tokens { refresh_token: "r0".into(), ..Tokens::bearer("old") };
    let body = r#"{"access_token":"new","refresh_token":"r1","expires_in":21600}"#;
    let fresh = grok().tokens(200, body, Some(&previous), Timestamp::now()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((fresh.access_token.as_str(), fresh.refresh_token.as_str()), ("new", "r1"));
}

#[test]
fn the_catalog_keeps_grok_chat_models_with_their_largest_window() {
    let page = json!({"data": [
        {"id": "grok-4.6", "context_windows": [256000, 500000]},
        {"id": "grok-4.20-0309-non-reasoning", "context_window": 2000000},
        {"id": "grok-imagine-image"},
        {"id": "grok-composer-2.5-fast", "supports_reasoning_effort": false},
        {"id": "other-model"}
    ]});
    let models = crate::catalog::collect(grok(), |_| Ok::<_, ProviderError>(page.clone())).unwrap_or_default();
    let ids: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "grok-subscription:grok-composer-2.5-fast",
            "grok-subscription:grok-4.6",
            "grok-subscription:grok-4.20-0309-non-reasoning"
        ]
    );
    let reasons = |i: usize| models[i].capabilities.as_ref().is_some_and(|c| c.reasoning);
    assert_eq!((reasons(0), reasons(1), reasons(2)), (false, true, false));
    assert_eq!((models[1].window, models[2].window), (Some(500000), Some(2000000)));
}

#[test]
fn the_body_carries_the_bare_model_and_xai_s_effort() {
    let mut body = json!({"model": "grok-subscription:grok-4.6", "reasoning": {"effort": "high", "summary": "auto"}});
    crate::request::shape(grok(), None, &mut body).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(body["model"], "grok-4.6");
    assert_eq!(body["reasoning"], json!({"effort": "high"}));
}

#[test]
fn the_code_is_asked_for_with_the_grok_scopes() {
    let (config, log) = signed(vec![code(5, 600), granted()]);
    assert!(config.is_ok(), "{config:?}");
    assert!(log.sent[0].1.starts_with(&format!("client_id={CLIENT}&scope=")), "{}", log.sent[0].1);
    assert!(log.sent[0].1.contains("api%3Aaccess"));
    assert!(log.sent[1].1.starts_with("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"));
}

#[test]
fn the_complete_link_is_opened_when_there_is_one() {
    let (config, log) = signed(vec![code(5, 600), granted()]);
    assert!(config.is_ok(), "{config:?}");
    assert_eq!(log.opened, [("https://accounts.x.ai/device?code=AB-12".to_owned(), Some("AB-12".to_owned()))]);
    assert_eq!(seconds(&log), [5]);
    let (refused, _) = signed(vec![(400, code(5, 600).1)]);
    assert!(refused.is_err());
    let plain = json!({"device_code": "d", "user_code": "u", "verification_uri": "http://x"});
    let (insecure, log) = signed(vec![polled(200, plain)]);
    assert!(insecure.is_err());
    assert!(log.opened.is_empty());
}

#[test]
fn polling_waits_slows_down_or_is_granted() {
    let replies = vec![
        code(5, 600),
        polled(400, json!({"error": "authorization_pending"})),
        polled(400, json!({"error": "slow_down"})),
        granted(),
    ];
    let (config, log) = signed(replies);
    assert!(config.is_ok(), "{config:?}");
    assert_eq!(seconds(&log), [5, 5, 10]);
    let denied = polled(400, json!({"error": "access_denied", "error_description": "denied"}));
    let (refused, _) = signed(vec![code(5, 600), denied]);
    assert_eq!(refused.map(|_| ()), Err(ProviderError::Unauthorized("denied".into())));
    let (slowest, log) = signed(vec![code(58, 600), polled(400, json!({"error": "slow_down"})), granted()]);
    assert!(slowest.is_ok(), "{slowest:?}");
    assert_eq!(seconds(&log), [58, 60]);
}

#[test]
fn the_device_flow_waits_for_approval_and_keeps_both_tokens() {
    let replies = vec![
        code(5, 600),
        polled(400, json!({"error": "authorization_pending"})),
        polled(400, json!({"error": "slow_down"})),
        granted(),
    ];
    let (config, log) = signed(replies);
    let config = config.unwrap_or_else(|e| panic!("{e}"));
    let tokens = config.refreshable().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((tokens.access_token.as_str(), tokens.refresh_token.as_str()), ("at", "rt"));
    assert_eq!(log.opened, [("https://accounts.x.ai/device?code=AB-12".to_owned(), Some("AB-12".to_owned()))]);
    assert_eq!(seconds(&log), [5, 5, 10]);
    let urls: Vec<_> = log.sent.iter().map(|(url, _)| url.as_str()).collect();
    assert_eq!(urls, [CODE, TOKEN, TOKEN, TOKEN]);
}

#[test]
fn a_refused_or_expired_code_ends_the_sign_in() {
    let denied = polled(400, json!({"error": "access_denied", "error_description": "denied"}));
    let (error, _) = signed(vec![code(5, 600), denied]);
    assert!(error.map(|_| ()).err().unwrap_or_else(|| panic!("denied")).to_string().contains("denied"));
    let pending = polled(400, json!({"error": "authorization_pending"}));
    let mut answers = vec![code(5, 60)];
    answers.extend(std::iter::repeat_n(pending, 12));
    let (error, _) = signed(answers);
    assert!(error.map(|_| ()).err().unwrap_or_else(|| panic!("expired")).to_string().contains("expired"));
}
