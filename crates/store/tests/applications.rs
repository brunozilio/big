use store::{Application, Binding, Bindings, Repository, Store};

fn application(id: &str, name: &str) -> Application {
    Application {
        id: id.into(),
        name: name.into(),
        bindings: Bindings::from([
            ("tracker".into(), Binding::new("c-tracker", "APP")),
            ("tests".into(), Binding::new("c-tests", "APP")),
            ("cloud".into(), Binding::new("c-cloud", "111/Deploy")),
        ]),
        repositories: vec![
            Repository {
                id: "r1".into(),
                directory: "/src/web".into(),
                run: vec!["bun dev".into()],
                verify: vec!["bun test".into(), "bun run lint".into()],
                bindings: Bindings::from([
                    ("code".into(), Binding::new("c-code", "acme/web")),
                    ("security".into(), Binding::new("c-security", "f1e2")),
                ]),
                ..Repository::default()
            },
            Repository { id: "r2".into(), directory: "/src/api".into(), read_only: true, ..Repository::default() },
        ],
        ..Application::default()
    }
}

#[test]
fn applications_are_kept_in_order_changed_in_place_and_removed() {
    let store = Store::in_memory(0).expect("the store answers");
    assert!(store.applications().expect("the store answers").is_empty());
    store.save_application(&application("a", "Web")).expect("saved");
    store.save_application(&application("b", "Api")).expect("saved");
    assert_eq!(store.application("b").expect("read"), Some(application("b", "Api")));
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
fn an_application_saved_with_the_old_dependabot_flag_still_reads() {
    let old: Application = serde_json::from_str(r#"{"id":"a","name":"Loja","repositories":[]}"#).expect("old data");
    assert_eq!(old.bound("cloud"), None, "agents keep the connection's own account and role");
    let off: Application = serde_json::from_str(r#"{"name":"Loja","skipDependabot":false}"#).expect("turned off");
    assert_eq!(off.name, "Loja");
}
