use std::sync::Arc;

use agent_engine::tools::{Workspace, clients, hosted};
use serde_json::json;
use store::{Application, CodeChoice, Repository, Store};

use crate::application::{ADD_REPOSITORY, live, offer};

fn stored(web: &str) -> Arc<Store> {
    let store = Arc::new(Store::in_memory(0).expect("store"));
    let application = Application {
        skip_dependabot: true,
        id: "app".into(),
        name: "Loja".into(),
        tracker: None,
        repositories: vec![Repository {
            id: "r1".into(),
            code: Some(CodeChoice { connection: "c-code".into(), repo: "acme/web".into() }),
            directory: web.into(),
            run: "bun dev".into(),
            verify: vec!["bun test".into()],
            e2e: vec!["npx playwright test".into()],
        }],
    };
    store.save_application(&application).expect("saved");
    store
}

fn call(store: &Arc<Store>, cwd: &std::path::Path, input: &serde_json::Value) -> agent_engine::tools::Outcome {
    let client = clients::fresh();
    offer(client, store, "app".into());
    let tools = hosted::tools(client);
    let tool = tools.iter().find(|tool| tool.spec().name == ADD_REPOSITORY).expect("offered");
    let outcome = tool.run(input, &Workspace::new(cwd));
    clients::release(client);
    outcome
}

#[test]
fn an_agent_adds_a_new_repository_to_its_application_with_the_application_s_code_connection() {
    let dir = tempfile::tempdir().expect("dir");
    let web = dir.path().join("web");
    let api = dir.path().join("api");
    std::fs::create_dir_all(&web).expect("web");
    std::fs::create_dir_all(&api).expect("api");
    let store = stored(&web.to_string_lossy());
    let input = json!({ "directory": "api", "repo": "acme/api", "run": "cargo run", "verify": ["cargo test", " "] });
    let outcome = call(&store, dir.path(), &input);
    assert!(!outcome.is_error, "{}", outcome.content);
    assert!(outcome.content.contains("Adicionei"));
    let application = store.application("app").expect("read").expect("there");
    let added = application.repositories.last().expect("added");
    assert_eq!(application.repositories.len(), 2);
    assert_eq!(added.directory, api.to_string_lossy());
    assert_eq!(added.code, Some(CodeChoice { connection: "c-code".into(), repo: "acme/api".into() }));
    assert_eq!((added.run.as_str(), added.verify.as_slice()), ("cargo run", ["cargo test".to_owned()].as_slice()));
    assert!(!added.id.is_empty() && added.id != "r1");
}

#[test]
fn a_known_directory_is_updated_in_place_and_keeps_what_was_left_out() {
    let dir = tempfile::tempdir().expect("dir");
    let store = stored(&dir.path().to_string_lossy());
    let input = json!({ "directory": dir.path().to_string_lossy(), "verify": ["bun test", "bun run lint"] });
    let outcome = call(&store, dir.path(), &input);
    assert!(outcome.content.contains("Atualizei"), "{}", outcome.content);
    let application = store.application("app").expect("read").expect("there");
    let [only] = application.repositories.as_slice() else { panic!("one repository") };
    assert_eq!((only.id.as_str(), only.run.as_str()), ("r1", "bun dev"));
    assert_eq!(only.verify, ["bun test", "bun run lint"]);
}

#[test]
fn a_directory_that_does_not_exist_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let store = stored("/src/web");
    let outcome = call(&store, dir.path(), &json!({ "directory": "nada" }));
    assert!(outcome.is_error);
    assert_eq!(store.application("app").expect("read").expect("there").repositories.len(), 1);
}

#[test]
fn later_steps_see_the_application_as_it_is_now_not_as_it_was_when_the_item_began() {
    let store = stored("/src/web");
    let input = json!({ "title": "x", "application": { "id": "app", "repositories": [] } });
    let seen = live(&store, &input);
    assert_eq!(seen["application"]["repositories"][0]["directory"], "/src/web");
    assert_eq!(seen["title"], "x");
    let other = json!({ "application": { "id": "gone", "repositories": [{ "id": "k" }] } });
    assert_eq!(live(&store, &other), other);
}
