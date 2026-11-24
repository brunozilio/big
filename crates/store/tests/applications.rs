use store::{Application, CloudChoice, CodeChoice, Repository, Store, TrackerChoice};

fn application(id: &str, name: &str) -> Application {
    Application {
        skip_dependabot: true,
        id: id.into(),
        name: name.into(),
        tracker: Some(TrackerChoice { connection: "c-tracker".into(), project: "APP".into() }),
        tests: Some(TrackerChoice { connection: "c-tests".into(), project: "APP".into() }),
        cloud: Some(CloudChoice { connection: "c-cloud".into(), account: "111".into(), role: "Deploy".into() }),
        repositories: vec![
            Repository {
                id: "r1".into(),
                code: Some(CodeChoice { connection: "c-code".into(), repo: "acme/web".into() }),
                directory: "/src/web".into(),
                run: vec!["bun dev".into()],
                verify: vec!["bun test".into(), "bun run lint".into()],
            },
            Repository { id: "r2".into(), code: None, directory: "/src/api".into(), run: vec![], verify: vec![] },
        ],
    }
}

#[test]
fn applications_are_kept_in_order_changed_in_place_and_removed() {
    let store = Store::in_memory(0).expect("the store answers");
    assert!(store.applications().expect("the store answers").is_empty());
    store.save_application(&application("a", "Web")).expect("saved");
    store.save_application(&application("b", "Api")).expect("saved");
    let mut changed = application("a", "Web nova");
    changed.repositories.pop();
    store.save_application(&changed).expect("saved");
    let names: Vec<String> = store.applications().expect("listed").into_iter().map(|found| found.name).collect();
    assert_eq!(names, ["Web nova", "Api"]);
    assert_eq!(store.application("a").expect("read"), Some(changed));
    assert!(store.delete_application("a").expect("removed"));
    assert!(!store.delete_application("a").expect("removed"));
    assert_eq!(store.application("a").expect("read"), None);
}

#[test]
fn an_item_keeps_the_workflow_code_it_was_pinned_to() {
    let store = Store::in_memory(0).expect("the store answers");
    assert_eq!(store.pinned_workflow("i1").expect("read"), None);
    store.pin_workflow("i1", "v1", "codigo 1").expect("pinned");
    store.pin_workflow("i2", "v1", "outro texto com a mesma versão").expect("pinned");
    assert_eq!(store.pinned_workflow("i1").expect("read"), Some(("v1".to_owned(), "codigo 1".to_owned())));
    assert_eq!(
        store.pinned_workflow("i2").expect("read"),
        Some(("v1".to_owned(), "codigo 1".to_owned())),
        "a version is kept once"
    );
}

#[test]
fn a_repository_saved_with_a_single_run_command_reads_it_as_a_list() {
    let old =
        r#"{ "directory": "/src/web", "run": "bun dev", "verify": ["bun test"], "e2e": ["npx playwright test"] }"#;
    let found: Repository = serde_json::from_str(old).expect("parsed");
    assert_eq!(found.run, ["bun dev"]);
    assert_eq!(found.verify, ["bun test"]);
    let blank: Repository = serde_json::from_str(r#"{ "directory": "/src/web", "run": "" }"#).expect("blank");
    assert!(blank.run.is_empty());
}

#[test]
fn an_application_saved_before_the_dependabot_flag_skips_its_runs() {
    let old: Application = serde_json::from_str(r#"{"id":"a","name":"Loja","repositories":[]}"#).expect("old data");
    assert!(old.skip_dependabot);
    assert_eq!(old.cloud, None, "agents keep the connection's own account and role");
    let off: Application = serde_json::from_str(r#"{"name":"Loja","skipDependabot":false}"#).expect("turned off");
    assert!(!off.skip_dependabot);
}
