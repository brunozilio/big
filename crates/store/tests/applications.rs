use store::{Application, CodeChoice, Repository, Store, TrackerChoice};

fn application(id: &str, name: &str) -> Application {
    Application {
        skip_dependabot: true,
        id: id.into(),
        name: name.into(),
        tracker: Some(TrackerChoice { connection: "c-tracker".into(), project: "APP".into() }),
        tests: Some(TrackerChoice { connection: "c-tests".into(), project: "APP".into() }),
        repositories: vec![
            Repository {
                id: "r1".into(),
                code: Some(CodeChoice { connection: "c-code".into(), repo: "acme/web".into() }),
                directory: "/src/web".into(),
                run: "bun dev".into(),
                verify: vec!["bun test".into(), "bun run lint".into()],
                e2e: vec!["npx playwright test".into()],
            },
            Repository {
                id: "r2".into(),
                code: None,
                directory: "/src/api".into(),
                run: String::new(),
                verify: vec![],
                e2e: vec![],
            },
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
fn a_repository_saved_before_e2e_existed_reads_with_no_e2e_commands() {
    let old = r#"{ "directory": "/src/web", "verify": ["bun test"] }"#;
    let found: Repository = serde_json::from_str(old).expect("parsed");
    assert!(found.e2e.is_empty());
    assert_eq!(found.verify, ["bun test"]);
}

#[test]
fn an_application_saved_before_the_dependabot_flag_skips_its_runs() {
    let old: Application = serde_json::from_str(r#"{"id":"a","name":"Loja","repositories":[]}"#).expect("old data");
    assert!(old.skip_dependabot);
    let off: Application = serde_json::from_str(r#"{"name":"Loja","skipDependabot":false}"#).expect("turned off");
    assert!(!off.skip_dependabot);
}
