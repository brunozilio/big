#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

use std::sync::Arc;

use agent_engine::fake::FakeEngine;
use app::api::{FeatureError, FeatureName};
use app::{Client, Error, Model};
use app::{InMemorySecretStore, LocalServices};

fn client(data: &std::path::Path, off: &[&str]) -> Client {
    let engine = FakeEngine::new(vec![Model::new("scripted", "Scripted")], Vec::new());
    let mut services = LocalServices::with_engine(Arc::new(engine));
    services.off.extend(off.iter().map(|feature| (*feature).to_owned()));
    services.homes = Arc::new(app::OwnerHome::at(data));
    services.providers = services.providers.with_secrets(Arc::new(InMemorySecretStore::default()));
    Client::builder().data_dir(data).services(services).build().expect(brand::COMMAND)
}

#[test]
fn a_feature_that_is_turned_off_is_an_error_of_its_own_that_says_how_to_turn_it_on() {
    let data = tempfile::tempdir().expect("data");
    let client = client(data.path(), &[domain::features::MEMORY_SEARCH, domain::features::SCHEDULES]);

    let refused = client.scheduled().expect_err("off");

    assert!(
        matches!(&refused, Error::Feature(FeatureError::FeatureOff { feature: FeatureName::Schedules })),
        "{refused:?}"
    );
    assert_eq!(refused.to_string(), "schedules is turned off (/features schedules on)");
    assert!(client.notices(true).expect("notices").is_empty());
}

#[test]
fn a_feature_that_fails_says_why_and_names_itself() {
    let data = tempfile::tempdir().expect("data");
    let client = client(data.path(), &[domain::features::MEMORY_SEARCH]);

    let failed = client.background_output("bash_none").expect_err("no such command");

    assert!(
        matches!(&failed, Error::Feature(FeatureError::Failed { feature: FeatureName::Shell, message })
            if message == "no background command bash_none"),
        "{failed:?}"
    );
}

#[test]
fn each_feature_answers_its_typed_call() {
    let data = tempfile::tempdir().expect("data");
    let client = client(data.path(), &[domain::features::MEMORY_SEARCH]);
    let cwd = data.path();

    assert!(client.sessions().expect("sessions").is_empty());
    assert!(client.memory(cwd).list().expect("memory").is_empty());
    assert!(client.project_files(cwd).expect("files").is_empty());
    assert_eq!(client.with_mentioned_files(cwd, "no files here").expect("mentions"), "no files here");
    assert!(client.git_offered(cwd).expect("offer"));
    assert!(client.background_commands().expect("commands").is_empty());
    let plugins = client.plugins(app::PluginsRequest::List).expect("plugins");
    assert_eq!(plugins, app::PluginsReply::Listed(Vec::new()));
    assert!(client.flags(cwd).expect("flags").iter().any(|listed| listed.flag.name == "budget" && listed.flag.on));
    assert!(client.scheduled().expect("scheduled").is_empty());
    assert!(client.take_due_schedules().expect("due").is_empty());
    assert!(client.keep_instruction(cwd, "Run the tests first.").is_ok());
    assert_eq!(client.trace_of_every_session().expect("trace")["sessions"], serde_json::json!([]));
    assert_eq!(client.open_channels(cwd, &[]).expect("channels"), (Vec::new(), Vec::new()));
    assert_eq!(client.code_map(cwd, "hello").expect("code map"), None);
    assert!(client.output_savings().expect("savings").is_empty());
}
