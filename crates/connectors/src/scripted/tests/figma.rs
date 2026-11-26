use serde_json::{Value, json};

use super::{configured, creds, loaded, now};
use crate::descriptor::AuthSpec;
use crate::error::{ActionError, ErrorClass};
use crate::http::{FakeHttp, Method};
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
fn figma_signs_in_with_a_personal_token_and_offers_the_design_capability() {
    let provider = loaded("figma");
    let descriptor = provider.descriptor();
    let AuthSpec::Token(spec) = descriptor.auth else { panic!("without a client id the token is the way in") };
    assert_eq!(spec.fields.len(), 1);
    assert!(spec.fields[0].secret);
    assert!(provider.alternatives(domain::Language::Portuguese).is_empty());
    let actions: Vec<&str> = descriptor.actions.iter().map(|action| action.name.as_str()).collect();
    assert_eq!(actions, ["design.frame"]);
}

#[tokio::test]
async fn a_frame_link_brings_its_texts_structure_and_rendered_image() {
    let http = FakeHttp::new().once(Method::Get, "/v1/files/AbC123/nodes", 200, NODES).once(
        Method::Get,
        "/v1/images/AbC123",
        200,
        r#"{"images":{"12:34":"https://img.test/12-34.png"}}"#,
    )
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

fn oauth() -> crate::scripted::ScriptedProvider {
    configured("figma", json!({ "client_id": "cid", "client_secret": "sec", "callback_port": "47800" }))
}

#[tokio::test]
async fn with_a_client_id_figma_signs_in_through_the_browser_and_keeps_the_token_as_an_option() {
    let provider = oauth();
    let AuthSpec::Loopback(sign_in) = provider.descriptor().auth else { panic!("not loopback") };
    assert_eq!(sign_in.0.callback_port(), Some(47800));
    let alternatives = provider.alternatives(domain::Language::Portuguese);
    assert!(matches!(alternatives.as_slice(), [only] if matches!(only.auth, AuthSpec::Token(_))));
    let http = FakeHttp::new();
    let begun = sign_in.0.begin(&http, 47800).await;
    let Ok(begun) = begun else { panic!("{begun:?}") };
    assert!(begun.url.starts_with("https://www.figma.com/oauth?client_id=cid&"), "{}", begun.url);
    for part in ["redirect_uri=http%3A%2F%2F127.0.0.1%3A47800%2Foauth%2Fcallback", "scope=file_content%3Aread+current_user%3Aread", "code_challenge_method=S256"] {
        assert!(begun.url.contains(part), "{part} missing in {}", begun.url);
    }
    let http = FakeHttp::new().once(Method::Post, "/v1/oauth/token", 200, r#"{"user_id":"7","access_token":"figu_a","refresh_token":"r1","expires_in":7776000}"#);
    let grant = sign_in.0.exchange(&http, &begun, "code1", now()).await;
    let Ok(grant) = grant else { panic!("{grant:?}") };
    assert_eq!((grant.access_token.as_str(), grant.refresh_token.as_deref()), ("figu_a", Some("r1")));
    let sent = http.sent();
    assert!(sent[0].headers.iter().any(|(name, value)| name == "authorization" && value == "Basic Y2lkOnNlYw=="));
    let body = String::from_utf8_lossy(&sent[0].body).into_owned();
    assert!(body.contains("code=code1") && body.contains("grant_type=authorization_code") && body.contains("code_verifier="), "{body}");
    let http = FakeHttp::new().once(Method::Post, "/v1/oauth/refresh", 200, r#"{"access_token":"figu_b","expires_in":7776000}"#);
    let renewed = sign_in.0.refresh(&http, &grant, now()).await;
    assert_eq!(renewed.map(|renewed| (renewed.access_token, renewed.refresh_token)), Ok(("figu_b".to_owned(), Some("r1".to_owned()))));
}

#[tokio::test]
async fn a_signed_in_account_calls_figma_with_a_bearer_token() {
    let http = FakeHttp::new().once(Method::Get, "/v1/me", 200, r#"{"id":"7","handle":"Ana"}"#);
    let found = oauth().accounts(&http, &creds("figu_a", json!({}))).await;
    assert!(found.is_ok(), "{found:?}");
    let sent = http.sent();
    assert!(sent[0].headers.iter().any(|(name, value)| name == "authorization" && value == "Bearer figu_a"));
    assert!(!sent[0].headers.iter().any(|(name, _)| name == "x-figma-token"));
}
