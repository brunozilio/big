use serde_json::{Value, json};

use super::{creds, loaded, now};
use crate::descriptor::AuthSpec;
use crate::error::{ActionError, ErrorClass};
use crate::http::{FakeHttp, HttpResponse, Method};
use crate::oauth::{DeviceCode, Polled, Started};
use crate::provider::Provider;

const PERSONAL: &str = r#"{"token":"figd_1"}"#;

async fn frame(http: &FakeHttp, url: &str) -> Result<Value, ActionError> {
    let provider = loaded("figma");
    let answer = provider.call(http, &creds(PERSONAL, json!({})), "design.frame", &json!({ "url": url }), "k").await;
    if let Ok(value) = &answer {
        let spec = provider.descriptor().action("design.frame").cloned().unwrap_or_else(|| panic!("no design.frame"));
        assert_eq!(crate::descriptor::conforms(&spec.output_schema, value), Ok(()), "{value}");
    }
    answer
}

const NODES: &str = r#"{"name":"App","nodes":{"12:34":{"document":{"id":"12:34","name":"Cadastro","type":"FRAME","children":[
    {"name":"Título","type":"TEXT","characters":"Novo cliente"},
    {"name":"Ajuda","type":"INSTANCE","children":[{"name":"tooltip","type":"TEXT","characters":"Informe o CPF sem pontos"}]},
    {"name":"oculto","type":"TEXT","characters":"rascunho","visible":false}]}}}}"#;

#[test]
fn figma_signs_in_with_the_session_and_offers_the_design_capability() {
    let provider = loaded("figma");
    let descriptor = provider.descriptor();
    assert!(matches!(descriptor.auth, AuthSpec::Steps(_)), "the session sign-in is the main way in");
    assert_eq!(provider.alternatives(domain::Language::Portuguese).len(), 1);
    let actions: Vec<&str> = descriptor.actions.iter().map(|action| action.name.as_str()).collect();
    assert_eq!(actions, ["design.frame"]);
}

#[tokio::test]
async fn a_frame_link_brings_its_texts_structure_and_rendered_image() {
    let http = FakeHttp::new()
        .once(Method::Get, "/v1/files/AbC123/nodes", 200, NODES)
        .once(Method::Get, "/v1/images/AbC123", 200, r#"{"images":{"12:34":"https://img.test/12-34.png"}}"#)
        .once(Method::Get, "/12-34.png", 200, "png");
    let found = frame(&http, "https://www.figma.com/design/AbC123/App?node-id=12-34&t=x").await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(found["ref"], "AbC123:12:34");
    assert_eq!(found["name"], "Cadastro");
    assert_eq!(found["file"], "App");
    assert_eq!(found["texts"], json!(["Novo cliente", "Informe o CPF sem pontos"]));
    let outline = found["outline"].as_str().unwrap_or_default();
    assert!(
        outline.starts_with("- Cadastro (frame)") && outline.contains("    - tooltip (text): Informe o CPF sem pontos"),
        "{outline}"
    );
    assert!(!outline.contains("oculto"), "{outline}");
    assert_eq!(found["image"], "https://img.test/12-34.png");
    assert_eq!(found["picture"], json!({ "media_type": "image/png", "data": "cG5n" }));
    let sent = http.sent();
    assert!(!sent[2].headers.iter().any(|(name, _)| name == "x-figma-token"), "the image host must not get the token");
    assert!(sent[0].url.contains("ids=12%3A34"), "{}", sent[0].url);
    assert!(sent[0].headers.iter().any(|(name, value)| name == "x-figma-token" && value == "figd_1"));
}

#[tokio::test]
async fn a_branch_link_reads_the_branch_and_a_file_link_reads_the_whole_file() {
    let file = r#"{"name":"App","document":{"name":"Document","type":"DOCUMENT","children":[{"name":"Tela","type":"CANVAS"}]}}"#;
    let http = FakeHttp::new().once(Method::Get, "/v1/files/Br4nch", 200, file);
    let found = frame(&http, "https://www.figma.com/design/AbC123/branch/Br4nch/App").await;
    assert_eq!(
        found.map(|found| (found["ref"].clone(), found["image"].clone(), found["picture"].clone())),
        Ok((json!("Br4nch"), Value::Null, Value::Null))
    );
    assert!(http.sent()[0].url.contains("depth=3"));
}

#[tokio::test]
async fn a_link_that_is_not_figma_is_refused_without_any_request() {
    let http = FakeHttp::new();
    let refused = frame(&http, "https://github.com/acme/web/pull/1").await;
    assert_eq!(refused.map_err(|error| error.class), Err(ErrorClass::Permanent));
    assert!(http.sent().is_empty());
}

#[tokio::test]
async fn a_refused_token_is_an_auth_error_and_a_valid_one_names_the_account() {
    let http = FakeHttp::new().once(Method::Get, "/v1/me", 403, r#"{"status":403,"err":"Invalid token"}"#);
    let refused = loaded("figma").accounts(&http, &creds("bad", json!({}))).await;
    assert_eq!(refused.map_err(|error| error.class), Err(ErrorClass::Auth));
    let http = FakeHttp::new().once(Method::Get, "/v1/me", 200, r#"{"id":"7","handle":"Ana","email":"ana@acme.test"}"#);
    let found = loaded("figma").accounts(&http, &creds(PERSONAL, json!({}))).await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(found[0].label, "Ana");
}

#[tokio::test]
async fn the_session_signs_in_without_a_browser_and_its_cookies_are_the_grant() {
    let AuthSpec::Steps(sign_in) = loaded("figma").descriptor().auth else { panic!("not steps") };
    let http = FakeHttp::new().respond(
        Method::Post,
        "/api/session/app_auth",
        HttpResponse::new(200, r#"{"meta":{"id":"abc","app_type":"desktop"}}"#)
            .with_header("set-cookie", "figma.session=s1; Path=/; HttpOnly, product_locale=%22en%22; Path=/"),
    );
    let started = sign_in.0.start(&http, &json!({}), domain::Language::Portuguese).await;
    let Ok(started) = started else { panic!("{started:?}") };
    let sent = http.sent();
    assert!(sent[0].body_text().contains("\"app_type\":\"desktop\""), "{:?}", sent[0].body_text());
    let http = FakeHttp::new()
        .once(Method::Get, "/api/session/state", 200, r#"{"meta":{"users":[{"id":"7","handle":"Ana"}]}}"#)
        .once(Method::Get, "/api/user/state?fuid=7", 200, r#"{"meta":{"team_users":[]}}"#);
    let polled = sign_in.0.poll(&http, &started, now()).await;
    let Ok(Polled::Authorized(grant)) = polled else { panic!("{polled:?}") };
    assert_eq!(grant.access_token, r#"{"cookies":"figma.session=s1; product_locale=%22en%22","fuid":"7"}"#);
    let sent = http.sent();
    assert!(
        sent.iter().all(|request| request.header_value("cookie") == Some("figma.session=s1; product_locale=%22en%22"))
    );
}

#[tokio::test]
async fn a_session_still_waiting_for_the_browser_sign_in_keeps_polling() {
    let AuthSpec::Steps(sign_in) = loaded("figma").descriptor().auth else { panic!("not steps") };
    let started = Started {
        code: DeviceCode {
            device_code: "abc".into(),
            user_code: String::new(),
            verification_uri: String::new(),
            verification_uri_complete: None,
            expires_in: 900,
            interval: 2,
        },
        carried: json!({ "cookies": "figma.session=s1" }),
    };
    for (status, body) in [(202, ""), (202, r#"{"meta":{"users":[{"id":"7"}]}}"#), (200, r#"{"meta":{"users":[]}}"#)] {
        let http = FakeHttp::new().once(Method::Get, "/api/session/state", status, body);
        let polled = sign_in.0.poll(&http, &started, now()).await;
        assert!(matches!(polled, Ok(Polled::Pending)), "{status} {body}: {polled:?}");
    }
}

#[tokio::test]
async fn a_refreshed_session_without_an_account_is_an_auth_error() {
    let AuthSpec::Steps(sign_in) = loaded("figma").descriptor().auth else { panic!("not steps") };
    let grant = crate::oauth::Grant {
        access_token: r#"{"cookies":"figma.session=s1","fuid":"7"}"#.into(),
        ..serde_json::from_value(json!({ "access_token": "" })).unwrap_or_else(|error| panic!("{error}"))
    };
    let http = FakeHttp::new().once(Method::Get, "/api/session/state", 200, r#"{"meta":{"users":[]}}"#);
    let refreshed = sign_in.0.refresh(&http, &grant, now()).await;
    assert_eq!(refreshed.map_err(|error| error.class), Err(ErrorClass::Auth));
}

#[tokio::test]
async fn a_session_account_calls_the_host_with_its_cookie() {
    let token = r#"{"cookies":"figma.session=s1","fuid":"7"}"#;
    let http = FakeHttp::new().once(
        Method::Get,
        "/api/session/state",
        200,
        r#"{"meta":{"users":[{"id":"7","handle":"Ana"}]}}"#,
    );
    let found = loaded("figma").accounts(&http, &creds(token, json!({}))).await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(found[0].label, "Ana");
    let sent = http.sent();
    assert!(sent[0].url.starts_with("https://www.figma.com/api/session/state"), "{}", sent[0].url);
    assert_eq!(sent[0].header_value("cookie"), Some("figma.session=s1"));
}

#[tokio::test]
async fn a_signed_in_account_calls_figma_with_a_bearer_token() {
    let http = FakeHttp::new().once(Method::Get, "/v1/me", 200, r#"{"id":"7","handle":"Ana"}"#);
    let found = loaded("figma").accounts(&http, &creds("figu_a", json!({}))).await;
    assert!(found.is_ok(), "{found:?}");
    let sent = http.sent();
    assert!(sent[0].headers.iter().any(|(name, value)| name == "authorization" && value == "Bearer figu_a"));
    assert!(!sent[0].headers.iter().any(|(name, _)| name == "x-figma-token"));
}
