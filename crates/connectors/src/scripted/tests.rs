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
