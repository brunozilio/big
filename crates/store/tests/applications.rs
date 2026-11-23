use store::{Application, CodeChoice, Repository, Store, TrackerChoice};

fn application(id: &str, name: &str) -> Application {
    Application {
        id: id.into(),
        name: name.into(),
        tracker: Some(TrackerChoice { connection: "c-tracker".into(), project: "APP".into() }),
        repositories: vec![
            Repository {
                id: "r1".into(),
                code: Some(CodeChoice { connection: "c-code".into(), repo: "acme/web".into() }),
                directory: "/src/web".into(),
                run: "bun dev".into(),
                verify: vec!["bun test".into(), "bun run lint".into()],
            },
            Repository {
                id: "r2".into(),
                code: None,
                directory: "/src/api".into(),
                run: String::new(),
                verify: vec![],
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
