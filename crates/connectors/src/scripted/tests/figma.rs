use serde_json::{Value, json};

use super::{creds, loaded};
use crate::descriptor::AuthSpec;
use crate::error::{ActionError, ErrorClass};
use crate::http::{FakeHttp, Method};
use crate::provider::Provider;

async fn frame(http: &FakeHttp, url: &str) -> Result<Value, ActionError> {
    let provider = loaded("figma");
    let answer = provider.call(http, &creds("figd_1", json!({})), "design.frame", &json!({ "url": url }), "k").await;
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
    let AuthSpec::Token(spec) = descriptor.auth else { panic!("not a token") };
    assert_eq!(spec.fields.len(), 1);
    assert!(spec.fields[0].secret);
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
    );
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
    let sent = http.sent();
    assert!(sent[0].url.contains("ids=12%3A34"), "{}", sent[0].url);
    assert!(sent[0].headers.iter().any(|(name, value)| name == "x-figma-token" && value == "figd_1"));
}

#[tokio::test]
async fn a_branch_link_reads_the_branch_and_a_file_link_reads_the_whole_file() {
    let file = r#"{"name":"App","document":{"name":"Document","type":"DOCUMENT","children":[{"name":"Tela","type":"CANVAS"}]}}"#;
    let http = FakeHttp::new().once(Method::Get, "/v1/files/Br4nch", 200, file);
    let found = frame(&http, "https://www.figma.com/design/AbC123/branch/Br4nch/App").await;
    assert_eq!(found.map(|found| (found["ref"].clone(), found["image"].clone())), Ok((json!("Br4nch"), Value::Null)));
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
    let found = loaded("figma").accounts(&http, &creds("figd_1", json!({}))).await;
    let Ok(found) = found else { panic!("{found:?}") };
    assert_eq!(found[0].label, "Ana");
}
