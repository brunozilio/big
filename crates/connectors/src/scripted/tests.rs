use domain::connection::ConnectionId;
use jiff::Timestamp;
use serde_json::json;

use super::*;
use crate::http::{FakeHttp, HttpResponse, Method};

mod aws;
mod datadog;
mod figma;
mod github;
mod jira;
mod mcp;
mod microsoft;
mod mock_changes;
mod quick;
mod sonarqube;
mod veracode;
mod zephyr;
mod zephyr_fake;

pub fn loaded(key: &str) -> ScriptedProvider {
    let entry = format!("{FOLDER}{key}/{ENTRY}");
    ScriptedProvider::load_in(None, &entry).unwrap_or_else(|error| panic!("{error}"))
}

pub fn now() -> Timestamp {
    Timestamp::from_second(1_000).unwrap_or(Timestamp::UNIX_EPOCH)
}

pub fn creds(token: &str, settings: Value) -> Credentials {
    Credentials::new(&ConnectionId::from("c1"), token, settings)
}

#[test]
fn every_built_in_connector_loads_with_a_logo_and_only_actions_of_its_capabilities() {
    let entries = content::listed_in(None, FOLDER);
    let entries: Vec<&String> = entries.iter().filter(|path| path.ends_with(ENTRY)).collect();
    assert!(!entries.is_empty());
    for entry in entries {
        let provider = ScriptedProvider::load_in(None, entry).unwrap_or_else(|error| panic!("{error}"));
        let descriptor = provider.descriptor();
        assert!(descriptor.logo.starts_with("<svg"), "{entry}");
        assert!(!descriptor.actions.is_empty(), "{entry}");
        for action in &descriptor.actions {
            let capability = action.capability();
            assert!(capability.is_some_and(|c| descriptor.capabilities.contains(&c)), "{entry}: {}", action.name);
        }
    }
}

#[tokio::test]
async fn an_error_thrown_by_the_script_keeps_its_class() {
    let provider = loaded("datadog");
    let http = FakeHttp::new().once(Method::Post, "/v1/mcp", 401, r#"{"error":"invalid_token"}"#);
    let refused = provider.accounts(&http, &creds("at", json!({}))).await;
    assert_eq!(refused.map_err(|error| error.class), Err(crate::error::ErrorClass::Auth));
    let unknown =
        provider.call(&http, &creds("at", json!({ "url": "https://x" })), "tracker.issue", &json!({}), "k").await;
    assert!(unknown.is_err_and(|error| error.message.contains("tracker.issue")));
    let _ = HttpResponse::new(200, "");
}

#[test]
fn the_presentation_runs_the_script_once_per_language() {
    let provider = loaded("jira");
    let started = std::time::Instant::now();
    let first = provider.presentation(Language::Portuguese);
    let ran = started.elapsed();
    let started = std::time::Instant::now();
    let again = provider.presentation(Language::Portuguese);
    let kept = started.elapsed();
    eprintln!("presentation: ran the script in {ran:?}, served again in {kept:?}");
    assert_eq!(first, again);
    assert_eq!(provider.shown.lock().map(|shown| shown.len()).unwrap_or_default(), 1);
    assert_ne!(provider.presentation(Language::English), first);
}

#[test]
fn connectors_load_together_in_the_listed_order() {
    let started = std::time::Instant::now();
    let together: Vec<ProviderKey> = discover().iter().map(|provider| provider.descriptor().key).collect();
    let took_together = started.elapsed();
    let started = std::time::Instant::now();
    let one_by_one: Vec<ProviderKey> = entries()
        .iter()
        .filter_map(|entry| ScriptedProvider::load(entry).ok())
        .map(|provider| provider.descriptor().key)
        .collect();
    let took_alone = started.elapsed();
    eprintln!("{} connectors: together {took_together:?}, one by one {took_alone:?}", together.len());
    assert!(!together.is_empty());
    assert_eq!(together, one_by_one);
}

#[test]
fn every_shipped_action_says_what_it_does_and_credentials_are_kept_secret() {
    for provider in discover() {
        let descriptor = provider.descriptor();
        for action in &descriptor.actions {
            assert!(!action.description.trim().is_empty(), "{}: {} has no description", descriptor.key, action.name);
            let gives_credentials = matches!(action.name.as_str(), "code.git_credentials" | "cloud.identity");
            assert_eq!(action.secret, gives_credentials, "{}: {}", descriptor.key, action.name);
        }
    }
}

#[test]
fn every_shipped_connector_loads_at_the_current_contract_and_declares_each_action_s_effects() {
    use domain::operation::{Kind, Visibility};
    for entry in entries() {
        let provider = ScriptedProvider::load(&entry).unwrap_or_else(|error| panic!("{entry}: {error}"));
        for action in &provider.descriptor().actions {
            assert!(!action.profile.effects.is_empty(), "{entry}: {} declares no effects", action.name);
            assert_eq!(action.writes, action.profile.acts(), "{entry}: {}", action.name);
            assert_eq!(action.hidden(), action.visibility == Visibility::Host, "{entry}: {}", action.name);
            if action.profile.effects.contains(&Kind::Pii) {
                assert!(action.profile.sensitive, "{entry}: {}", action.name);
            }
        }
    }
}

const FIXTURE: &str = r#"import { looks, object, read, write } from "sdk/connectors/contracts.ts";
export function provider() {
  const share = { effects: ["send-external", "pii"], risk: "medium", reversible: false, untrusted: false, sensitive: true, recipients: ["to"], freeText: ["body"], permission: { default: "ask", alwaysAsk: true } };
  return { schema_version: 2, name: "Fixture", capabilities: ["notes"], auth: { kind: "token", fields: [] },
    actions: [write("notes.share", object([["to", "array"], ["body", "string"]]), object([]), "native", share), read("notes.list", object([]), object([]), looks(true))] };
}
export function call() { return null; }
"#;

fn installed(name: &str, source: &str) -> (tempfile::TempDir, Result<ScriptedProvider, String>) {
    let folder = tempfile::tempdir().expect("folder");
    let dir = folder.path().join(format!("connectors/{name}"));
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("index.ts"), source).expect("index");
    let loaded = ScriptedProvider::load_in(Some(folder.path()), &format!("connectors/{name}/index.ts"));
    (folder, loaded)
}

#[test]
fn a_connector_declares_what_an_action_sends_and_to_whom() {
    use domain::operation::Kind;
    let (_folder, fixture) = installed("fixture", FIXTURE);
    let fixture = fixture.unwrap_or_else(|error| panic!("{error}"));
    let share = fixture.descriptor().action("notes.share").cloned().expect("notes.share");
    assert_eq!(share.profile.effects, [Kind::SendExternal, Kind::Pii]);
    assert_eq!((share.recipients.as_slice(), share.permission.always_ask), (["to".to_owned()].as_slice(), true));
    let list = fixture.descriptor().action("notes.list").cloned().expect("notes.list");
    assert!(list.profile.untrusted && !list.profile.acts());
}

#[test]
fn a_first_contract_connector_is_brought_up_by_the_sdk_and_still_loads() {
    let old = "export function provider() {\n  return { schema_version: 1, name: \"Old\", capabilities: [\"x\"], auth: { kind: \"token\", fields: [] },\n    actions: [{ name: \"x.send\", input_schema: { type: \"object\" }, output_schema: {}, writes: true, idempotent: \"native\" }] };\n}\nexport function call() { return null; }\n";
    let (_folder, loaded) = installed("old", old);
    let send = loaded.unwrap_or_else(|error| panic!("{error}")).descriptor().actions[0].clone();
    assert_eq!(send.profile.effects, [domain::operation::Kind::Write]);
}

#[test]
fn a_connector_whose_write_declares_no_effects_does_not_load() {
    let source = "export function provider() {\n  return { schema_version: 2, name: \"Bare\", capabilities: [\"x\"], auth: { kind: \"token\", fields: [] },\n    actions: [{ name: \"x.send\", input_schema: { type: \"object\" }, output_schema: {}, writes: true, idempotent: \"native\", risk: \"high\", reversible: false, untrusted: false, sensitive: false }] };\n}\nexport function call() { return null; }\n";
    let (_folder, loaded) = installed("bare", source);
    let refused = loaded.err().unwrap_or_default();
    assert!(refused.contains("x.send writes but declares no effects"), "{refused}");
}
