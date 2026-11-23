#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

use std::sync::Arc;

use agent_engine::fake::FakeEngine;
use app::api::{FeatureError, FeatureName};
use app::{Client, Error, Model};
use app::{InMemorySecretStore, LocalServices};

fn client(data: &std::path::Path) -> Client {
    let engine = FakeEngine::new(vec![Model::new("scripted", "Scripted")], Vec::new());
    let mut services = LocalServices::with_engine(Arc::new(engine));
    services.homes = Arc::new(app::OwnerHome::at(data));
    services.providers = services.providers.with_secrets(Arc::new(InMemorySecretStore::default()));
    Client::builder().data_dir(data).services(services).build().expect(brand::COMMAND)
}

#[test]
fn a_feature_is_off_only_when_the_config_turns_it_off() {
    let data = tempfile::tempdir().expect("data");
    let client = client(data.path());

    let scheduled = client.scheduled();

    match scheduled {
        Ok(_) => assert!(FeatureName::Schedules.on()),
        Err(refused) => {
            assert!(!FeatureName::Schedules.on());
            assert!(matches!(&refused, Error::Feature(FeatureError::FeatureOff { feature: FeatureName::Schedules })));
            assert_eq!(refused.to_string(), "schedules is turned off");
        }
    }
    assert!(client.notices(true).expect("notices").is_empty());
}

#[test]
fn a_feature_that_fails_says_why_and_names_itself() {
    let data = tempfile::tempdir().expect("data");
    let client = client(data.path());

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
    let client = client(data.path());
    let cwd = data.path();

    assert!(client.sessions().expect("sessions").is_empty());
    assert!(client.memory(cwd).list().expect("memory").is_empty());
    assert!(client.project_files(cwd).expect("files").is_empty());
    assert_eq!(client.with_mentioned_files(cwd, "no files here").expect("mentions"), "no files here");
    assert_eq!(client.git_offered(cwd).expect("offer"), domain::features::on(brand::features::GIT_OFFER));
    assert!(client.background_commands().expect("commands").is_empty());
    let plugins = client.plugins(app::PluginsRequest::List).expect("plugins");
    assert_eq!(plugins, app::PluginsReply::Listed(Vec::new()));
    assert!(client.scheduled().expect("scheduled").is_empty());
    assert!(client.take_due_schedules().expect("due").is_empty());
    assert!(client.keep_instruction(cwd, "Run the tests first.").is_ok());
    assert_eq!(client.trace_of_every_session().expect("trace")["sessions"], serde_json::json!([]));
    assert_eq!(client.open_channels(cwd, &[]).expect("channels"), (Vec::new(), Vec::new()));
    assert_eq!(client.code_map(cwd, "hello").expect("code map"), None);
    assert!(client.output_savings().expect("savings").is_empty());
}
