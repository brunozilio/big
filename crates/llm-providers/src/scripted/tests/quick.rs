use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use domain::{Language, ModelOption};
use serde_json::{Value, json};

use super::provider;
use crate::testing::ScriptedHost;
use crate::{Config, Provider, ProviderError, Tokens};

const TENANT: &str = "https://qbs-cell1.dp.appintegrations.us-east-1.prod.plato.ai.aws.dev";
const DISCOVERED: &str = "nonce=r1x32&account=816885297016&issuerUrl=https%3A%2F%2Fidentitycenter.amazonaws.com%2Fssoins-1\
     &tenantUrl=https%3A%2F%2Fqbs-cell1.dp.appintegrations.us-east-1.prod.plato.ai.aws.dev&idcInstanceId=ssoins-1&idcRegion=us-east-1";

fn quick() -> &'static dyn Provider {
    provider("quick-subscription")
}

fn client() -> Value {
    json!({ "region": "us-east-1", "client_id": "cid", "client_secret": "secret", "tenant_url": TENANT, "issuer_url": "https://identitycenter.amazonaws.com/ssoins-1" })
}

fn signed_in() -> Config {
    let tokens = Tokens {
        refresh_token: "r".into(),
        account_id: "816885297016".into(),
        client: Some(client()),
        ..Tokens::bearer("a")
    };
    Config { auth: Some(tokens), ..Config::default() }
}

fn body(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_default()
}

fn signed(
    replies: Vec<(u16, String)>,
    visits: Vec<(&'static str, &'static str)>,
) -> (Result<Tokens, ProviderError>, crate::testing::Log) {
    let host = ScriptedHost::new(replies, visits);
    let tokens = quick().sign_in(&host).and_then(|config| config.refreshable().cloned());
    let log = std::mem::take(&mut *host.log.borrow_mut());
    (tokens, log)
}

#[test]
fn signing_in_discovers_the_account_then_signs_in_to_its_identity_center() {
    let replies = vec![
        (200, r#"{"clientId":"cid","clientSecret":"secret"}"#.to_owned()),
        (200, r#"{"accessToken":"a","refreshToken":"r","expiresIn":3600}"#.to_owned()),
    ];
    let (tokens, log) = signed(replies, vec![("/callback", DISCOVERED), ("/", "code=c&state=r3x32")]);
    let Ok(tokens) = tokens else { panic!("{tokens:?}") };
    assert_eq!(log.opened, [("https://quick.aws.com/sn?qwSurface=desktop%3Ar1x32%3A7%3A1.2.4759".to_owned(), None)]);
    let (registered, asked) = &log.sent[0];
    assert_eq!(registered, "https://oidc.us-east-1.amazonaws.com/client/register");
    let asked = body(asked);
    assert_eq!(asked["issuerUrl"], "https://identitycenter.amazonaws.com/ssoins-1");
    assert_eq!(asked["redirectUris"], json!(["http://127.0.0.1:7"]));
    assert_eq!(body(&log.sent[1].1)["redirectUri"], "http://127.0.0.1:7");
    assert_eq!(asked["scopes"], json!(["quicksight:read_write"]));
    let (exchanged, sent) = &log.sent[1];
    assert_eq!(exchanged, "https://oidc.us-east-1.amazonaws.com/token");
    assert_eq!(body(sent)["codeVerifier"], "r2x64");
    assert_eq!(body(sent)["grantType"], "authorization_code");
    assert_eq!((tokens.access_token.as_str(), tokens.account_id.as_str()), ("a", "816885297016"));
    assert_eq!(tokens.client.as_ref().map(|client| client["tenant_url"].clone()), Some(json!(TENANT)));
    assert!(log.answered[0].contains("https://oidc.us-east-1.amazonaws.com/authorize?"), "{:?}", log.answered);
    assert!(log.answered[0].contains("client_id=cid") && log.answered[0].contains("state=r3x32"), "{:?}", log.answered);
}

#[test]
fn a_discovery_without_a_region_is_refused_without_registering() {
    let (tokens, log) =
        signed(Vec::new(), vec![("/callback", "nonce=r1x32&tenantUrl=https%3A%2F%2Fx&issuerUrl=https%3A%2F%2Fy")]);
    assert!(matches!(tokens, Err(ProviderError::Unauthorized(_))), "{tokens:?}");
    assert!(log.sent.is_empty());
    assert!(log.answered[0].starts_with("failed"), "{:?}", log.answered);
}

#[test]
fn the_chat_is_a_socket_on_the_accounts_cell_with_the_token() {
    let config = signed_in();
    let base = config.base(quick());
    assert!(base.starts_with("wss://qbs-cell1.dp.appintegrations.us-east-1.prod.plato.ai.aws.dev/streaming/quickwork-chat-bidirectional-stream?"), "{base}");
    assert!(base.contains("surfaceType=QUICK_WORK_DESKTOP_MAC&token=a&workingSessionId="), "{base}");
    assert_eq!(quick().wire(None), "socket");
    assert_eq!(quick().framing(), "aws-eventstream");
}

fn inner(frame: &Value) -> Value {
    let outer = body(frame["binary"].as_str().unwrap_or_default());
    let decoded = STANDARD.decode(outer["payload"]["payload"].as_str().unwrap_or_default()).unwrap_or_default();
    assert_eq!(outer["eventType"], "rawInputEvent");
    serde_json::from_slice(&decoded).unwrap_or_default()
}

#[test]
fn a_turn_connects_then_sends_the_conversation_in_the_chosen_mode() {
    let mut sent = json!({
        "model": "quick-subscription:fast",
        "system": "Be brief.",
        "messages": [{ "role": "user", "text": "hi" }, { "role": "assistant", "text": "hello" }, { "role": "user", "text": "how are you?" }],
    });
    let model = ModelOption::new("quick-subscription:fast", "fast");
    quick().shape(Some(&model), &mut sent, "high").unwrap_or_else(|error| panic!("{error}"));
    let frames = sent["send"].as_array().cloned().unwrap_or_default();
    assert_eq!(frames.len(), 2);
    assert_eq!(inner(&frames[0])["type"], "session_connect");
    let message = inner(&frames[1]);
    assert_eq!(message["type"], "user_message");
    assert_eq!(message["payload"]["agent_mode"], "fast");
    let text = message["payload"]["text"].as_str().unwrap_or_default();
    assert!(text.starts_with("<instructions>\nBe brief.\n</instructions>"), "{text}");
    assert!(text.contains("<user>\nhi\n</user>\n<assistant>\nhello\n</assistant>"), "{text}");
    assert!(text.ends_with("how are you?"), "{text}");
}

fn shaped(kept: Value) -> Vec<Value> {
    let mut sent = json!({
        "model": "quick-subscription:smart",
        "system": "Be brief.",
        "conversation": "c1",
        "kept": kept,
        "messages": [{ "role": "user", "text": "hi" }, { "role": "assistant", "text": "hello" }, { "role": "user", "text": "how are you?" }],
    });
    quick().shape(None, &mut sent, "").unwrap_or_else(|error| panic!("{error}"));
    sent["send"].as_array().cloned().unwrap_or_default().iter().map(inner).collect()
}

#[test]
fn a_later_turn_continues_the_thread_with_only_what_is_new() {
    let frames = shaped(json!({ "value": { "thread": "t1" }, "sent": 2 }));
    assert_eq!(frames[1]["payload"]["thread_id"], "t1");
    assert_eq!(frames[1]["payload"]["text"], "how are you?");
}

#[test]
fn without_a_thread_or_with_a_rewritten_conversation_a_new_thread_gets_everything() {
    for kept in [Value::Null, json!({ "value": { "thread": "t1" }, "sent": 3 }), json!({ "value": {}, "sent": 2 })] {
        let frames = shaped(kept.clone());
        assert_ne!(frames[1]["payload"]["thread_id"], "t1", "{kept}");
        let text = frames[1]["payload"]["text"].as_str().unwrap_or_default();
        assert!(text.starts_with("<instructions>") && text.contains("<assistant>\nhello"), "{kept}: {text}");
    }
}

#[test]
fn the_end_of_a_turn_keeps_its_thread() {
    let done = read(&json!({ "type": "message_complete", "scope": "chat", "thread_id": "t9", "payload": {} }));
    assert_eq!((done["done"].clone(), done["keep"].clone()), (json!(true), json!({ "thread": "t9" })));
}

fn received(event: &Value) -> Value {
    let payload = json!({ "payload": STANDARD.encode(event.to_string()) }).to_string();
    json!({ "headers": { ":event-type": "rawOutputEvent", ":message-type": "event" }, "payload": payload })
}

fn read(event: &Value) -> Value {
    quick().frame(&Value::Null, &received(event)).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn frames_become_text_usage_and_the_end_of_the_turn() {
    assert_eq!(read(&json!({ "type": "text_delta", "scope": "chat", "payload": { "delta": "ok" } }))["text"], "ok");
    assert_eq!(
        read(&json!({ "type": "thinking_delta", "scope": "chat", "payload": { "delta": "hm" } }))["thinking"],
        "hm"
    );
    let usage = read(&json!({ "type": "turn_usage", "scope": "chat",
        "payload": { "input_tokens": 4, "output_tokens": 2, "cache_read_tokens": 30, "cache_write_tokens": 7, "served_model_id": "us.anthropic.claude-haiku" } }));
    assert_eq!(usage["usage"], json!({ "input": 4, "output": 2, "cache_read": 30, "cache_write": 7 }));
    assert_eq!(usage["served"], "us.anthropic.claude-haiku");
    assert_eq!(read(&json!({ "type": "message_complete", "scope": "chat", "payload": {} }))["done"], true);
    assert_eq!(
        read(&json!({ "type": "message_complete", "scope": "chat", "payload": { "continuation_expected": true } }))["done"],
        Value::Null
    );
    assert_eq!(
        read(&json!({ "type": "text_delta", "scope": "task_lifecycle", "payload": { "delta": "x" } }))["text"],
        Value::Null
    );
    assert_eq!(read(&json!({ "type": "session_established" }))["text"], Value::Null);
}

#[test]
fn errors_in_frames_are_classified() {
    let limited =
        read(&json!({ "type": "error", "payload": { "error_code": "usage_limit", "message": "out of credits" } }));
    assert_eq!(limited["error"], json!({ "kind": "overloaded", "message": "out of credits" }));
    let long = read(&json!({ "type": "error", "payload": { "error_code": "context_overflow", "message": "" } }));
    assert_eq!(long["error"]["kind"], "too_long");
    let refused = json!({ "headers": { ":message-type": "exception", ":exception-type": "AccessDenied" }, "payload": "{\"message\":\"no\"}" });
    let failed = quick().frame(&Value::Null, &refused).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(failed["error"]["kind"], "provider");
}

fn usage() -> Value {
    let body = json!({
        "session": { "used": 10, "limit": 100, "resets_at": null },
        "monthly": { "used": 150, "limit": 600, "resets_at": 1_796_083_200 },
        "subscription": { "subscription_type": "QUICK_LITE_FREE_TRIAL" },
    });
    let answer = json!({ "type": "api_response", "status": 200, "body": body.to_string() });
    json!({ "errorMessage": null, "eventResponse": { "invokeAgentResponse": answer.to_string() } })
}

#[test]
fn the_usage_of_the_session_and_month_are_the_plan_limits() {
    assert!(quick().limits(&Config::default()).is_some(), "the plan limits are offered before signing in");
    let call = quick().limits(&signed_in()).unwrap_or_else(|| panic!("no limits"));
    assert_eq!(call.url, format!("{TENANT}/integration/quick-work/qbs-event"));
    assert_eq!(body(call.body.as_deref().unwrap_or_default())["eventBody"]["invokeAgent"]["path"], "profile/usage");
    let usage = quick().windows(&usage(), Language::Portuguese).unwrap_or_else(|error| panic!("{error}"));
    let shown: Vec<(String, f64)> =
        usage.windows.iter().map(|window| (window.label.clone(), window.used_percent)).collect();
    assert_eq!(shown, [("Sessão · 10/100".to_owned(), 10.0), ("Mês · 150/600".to_owned(), 25.0)]);
}

#[test]
fn the_catalog_offers_the_modes_once_the_account_answers() {
    let models: Vec<String> = quick()
        .page(&usage())
        .unwrap_or_else(|error| panic!("{error}"))
        .models
        .into_iter()
        .map(|model| model.id)
        .collect();
    assert_eq!(models, ["quick-subscription:fast", "quick-subscription:smart", "quick-subscription:advanced"]);
}

#[test]
fn the_refresh_goes_to_the_identity_center_that_signed_in() {
    let previous = signed_in().auth.unwrap_or_else(|| panic!("no tokens"));
    let request = quick().refresh(&previous).unwrap_or_else(|| panic!("no refresh"));
    assert_eq!(request.url, "https://oidc.us-east-1.amazonaws.com/token");
    assert_eq!(body(&request.body)["grantType"], "refresh_token");
    let renewed = quick()
        .tokens(200, r#"{"accessToken":"b","expiresIn":3600}"#, Some(&previous), jiff::Timestamp::UNIX_EPOCH)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        (renewed.access_token.as_str(), renewed.refresh_token.as_str(), renewed.account_id.as_str()),
        ("b", "r", "816885297016")
    );
}
