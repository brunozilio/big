use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use super::{creds, loaded, now};
use crate::descriptor::AuthSpec;
use crate::error::ErrorClass;
use crate::http::{FakeHttp, Method};
use crate::oauth::{Authorization, Loopback, Proceeded};
use crate::provider::Provider;

const DISCOVERED: &str = "https://identitycenter.amazonaws.com/ssoins-1";
const TENANT: &str = "https://qbs-cell1.dp.appintegrations.us-east-1.prod.plato.ai.aws.dev";

fn sign_in() -> Loopback {
    let AuthSpec::Loopback(sign_in) = loaded("quick").descriptor().auth else { panic!("not loopback") };
    sign_in
}

fn query(nonce: &str) -> Value {
    json!({ "nonce": nonce, "account": "816885297016", "issuerUrl": DISCOVERED, "tenantUrl": TENANT, "idcInstanceId": "ssoins-1", "idcRegion": "us-east-1" })
}

fn body(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_default()
}

#[test]
fn quick_answers_questions_as_an_assistant_and_signs_in_on_its_own_port() {
    let provider = loaded("quick");
    let descriptor = provider.descriptor();
    let actions: Vec<&str> = descriptor.actions.iter().map(|action| action.name.as_str()).collect();
    assert_eq!(actions, ["assistant.ask"]);
    assert_eq!(sign_in().0.callback_port(), Some(18093));
}

async fn discovered() -> (Authorization, Authorization, FakeHttp) {
    let http = FakeHttp::new()
        .once(Method::Post, "/client/register", 200, r#"{"clientId":"cid","clientSecret":"secret"}"#)
        .once(Method::Post, "/token", 200, r#"{"accessToken":"at","refreshToken":"rt","expiresIn":3600}"#);
    let begun = sign_in().0.begin(&http, 7420).await.unwrap_or_else(|error| panic!("{error:?}"));
    let next = sign_in().proceed(&http, &begun, &query(&begun.state), now()).await;
    let Ok(Proceeded::Redirect(next)) = next else { panic!("{next:?}") };
    (begun, next, http)
}

#[tokio::test]
async fn the_sign_in_discovers_the_account_then_registers_and_authorizes_at_its_identity_center() {
    let (begun, next, http) = discovered().await;
    assert!(begun.url.starts_with("https://quick.aws.com/sn?qwSurface=desktop%3A"), "{}", begun.url);
    assert!(begun.url.ends_with("%3A18093%3A1.2.4759"), "{}", begun.url);
    let registered = body(&http.sent()[0].body_text());
    assert_eq!(registered["issuerUrl"], DISCOVERED);
    assert_eq!(registered["redirectUris"], json!(["http://127.0.0.1:18093"]));
    assert!(next.url.starts_with("https://oidc.us-east-1.amazonaws.com/authorize?"), "{}", next.url);
    assert!(next.url.contains("client_id=cid") && next.url.contains("code_challenge_method=S256"), "{}", next.url);
    assert_eq!(next.redirect_uri, "http://127.0.0.1:18093");
    let grant = sign_in().0.exchange(&http, &next, "code1", now()).await.unwrap_or_else(|error| panic!("{error:?}"));
    let exchanged = body(&http.sent()[1].body_text());
    assert_eq!((exchanged["code"].clone(), exchanged["codeVerifier"].clone()), (json!("code1"), json!(next.verifier)));
    let held = body(&grant.access_token);
    assert_eq!(held, json!({ "token": "at", "tenant_url": TENANT, "account": "816885297016" }));
    assert_eq!(grant.refresh_token.as_deref(), Some("rt"));
}

#[tokio::test]
async fn an_answer_for_another_sign_in_is_refused_before_registering() {
    let http = FakeHttp::new();
    let begun = sign_in().0.begin(&http, 1).await.unwrap_or_else(|error| panic!("{error:?}"));
    let refused = sign_in().proceed(&http, &begun, &query("other"), now()).await;
    assert!(matches!(refused, Err(ref error) if error.class == ErrorClass::Auth), "{refused:?}");
    assert!(http.sent().is_empty());
}

#[tokio::test]
async fn a_refresh_keeps_the_account_and_the_refresh_token() {
    let (_, next, http) = discovered().await;
    let grant = sign_in().0.exchange(&http, &next, "code1", now()).await.unwrap_or_else(|error| panic!("{error:?}"));
    let renewing = FakeHttp::new().once(Method::Post, "/token", 200, r#"{"accessToken":"at2","expiresIn":3600}"#);
    let renewed = sign_in().0.refresh(&renewing, &grant, now()).await.unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(body(&renewed.access_token)["token"], "at2");
    assert_eq!(renewed.refresh_token.as_deref(), Some("rt"));
    assert_eq!(body(&renewing.sent()[0].body_text())["grantType"], "refresh_token");
}

fn framed(event: &Value) -> Vec<u8> {
    let payload = json!({ "payload": STANDARD.encode(event.to_string()) }).to_string().into_bytes();
    let mut headers = Vec::new();
    for (name, value) in [(":event-type", "rawOutputEvent"), (":message-type", "event")] {
        headers.push(u8::try_from(name.len()).unwrap_or_default());
        headers.extend_from_slice(name.as_bytes());
        headers.push(7);
        headers.extend_from_slice(&u16::try_from(value.len()).unwrap_or_default().to_be_bytes());
        headers.extend_from_slice(value.as_bytes());
    }
    let total = 12 + headers.len() + payload.len() + 4;
    let mut out = Vec::new();
    out.extend_from_slice(&u32::try_from(total).unwrap_or_default().to_be_bytes());
    out.extend_from_slice(&u32::try_from(headers.len()).unwrap_or_default().to_be_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&headers);
    out.extend_from_slice(&payload);
    out.extend_from_slice(&[0; 4]);
    out
}

fn sent_inner(message: &Message) -> Value {
    let Message::Binary(bytes) = message else { return Value::Null };
    let outer: Value = serde_json::from_slice(bytes).unwrap_or_default();
    let inner = STANDARD.decode(outer["payload"]["payload"].as_str().unwrap_or_default()).unwrap_or_default();
    serde_json::from_slice(&inner).unwrap_or_default()
}

#[allow(clippy::result_large_err)]
async fn quick_cell(events: Vec<Value>) -> (String, tokio::task::JoinHandle<(String, Vec<Value>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap_or_else(|error| panic!("{error}"));
    let address = listener.local_addr().unwrap_or_else(|error| panic!("{error}"));
    let served = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap_or_else(|error| panic!("{error}"));
        let mut path = String::new();
        let callback = |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
            path = request.uri().to_string();
            Ok(response)
        };
        let mut socket =
            tokio_tungstenite::accept_hdr_async(stream, callback).await.unwrap_or_else(|error| panic!("{error}"));
        let mut heard = Vec::new();
        for _ in 0..2 {
            if let Some(Ok(message)) = socket.next().await {
                heard.push(sent_inner(&message));
            }
        }
        for event in events {
            let _ = socket.send(Message::Binary(framed(&event).into())).await;
        }
        if let Some(Ok(message)) = socket.next().await {
            heard.push(sent_inner(&message));
        }
        (path, heard)
    });
    (format!("http://{address}"), served)
}

#[tokio::test]
async fn a_question_is_answered_with_its_sources_on_the_thread_asked() {
    let events = vec![
        json!({ "type": "session_established" }),
        json!({ "type": "tool_call", "payload": { "tool_call_id": "t1", "tool_name": "file_read", "arguments": {} } }),
        json!({ "type": "text_delta", "scope": "task_lifecycle", "payload": { "delta": "noise" } }),
        json!({ "type": "text_delta", "scope": "chat", "payload": { "delta": "The launch is on Friday." } }),
        json!({ "type": "text_delta", "scope": "chat", "payload": { "delta": "\n<sources>\n[Launch plan](https://docs/plan)\n</sources>" } }),
        json!({ "type": "turn_usage", "scope": "chat", "payload": { "served_model_id": "us.anthropic.claude-haiku" } }),
        json!({ "type": "message_complete", "scope": "chat", "payload": {} }),
    ];
    let (tenant, served) = quick_cell(events).await;
    let token = json!({ "token": "at", "tenant_url": tenant, "account": "1" }).to_string();
    let asked =
        json!({ "question": "When is the launch?", "context": "Project Atlas", "thread": "th-1", "mode": "fast" });
    let answer = loaded("quick").call(&FakeHttp::new(), &creds(&token, json!({})), "assistant.ask", &asked, "k").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer["answer"], "The launch is on Friday.");
    assert_eq!(answer["sources"], json!([{ "title": "Launch plan", "url": "https://docs/plan" }]));
    assert_eq!(
        (answer["thread"].clone(), answer["model"].clone()),
        (json!("th-1"), json!("us.anthropic.claude-haiku"))
    );
    let (path, heard) = served.await.unwrap_or_else(|error| panic!("{error}"));
    assert!(
        path.starts_with("/streaming/quickwork-chat-bidirectional-stream?surfaceType=QUICK_WORK_DESKTOP_MAC&token=at&"),
        "{path}"
    );
    assert_eq!(heard[0]["type"], "session_connect");
    assert_eq!(heard[1]["payload"]["thread_id"], "th-1");
    assert_eq!(heard[1]["payload"]["agent_mode"], "fast");
    assert_eq!(heard[1]["payload"]["text"], "<context>\nProject Atlas\n</context>\n\nWhen is the launch?");
    assert_eq!(
        (heard[2]["type"].clone(), heard[2]["payload"]["tool_call_id"].clone()),
        (json!("tool_result"), json!("t1"))
    );
}

#[tokio::test]
async fn an_exhausted_plan_is_told_as_a_rate_limit() {
    let events =
        vec![json!({ "type": "error", "payload": { "error_code": "usage_limit", "message": "out of credits" } })];
    let (tenant, _served) = quick_cell(events).await;
    let token = json!({ "token": "at", "tenant_url": tenant, "account": "1" }).to_string();
    let refused = loaded("quick")
        .call(&FakeHttp::new(), &creds(&token, json!({})), "assistant.ask", &json!({ "question": "hi" }), "k")
        .await;
    assert!(matches!(refused, Err(ref error) if matches!(error.class, ErrorClass::RateLimited { .. })), "{refused:?}");
}
