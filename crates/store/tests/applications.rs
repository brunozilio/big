#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use common::Fresh;
use database::args;
use store::{Application, Binding, Bindings, Repository};

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

fn applications_are_kept_in_order_changed_in_place_and_removed(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
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

const SAVED_BEFORE_SPACES: &str = r#"{"id":"old","name":"Loja","tracker":{"connection":"c-tracker","project":"APP"},
    "repositories":[{"id":"r1","directory":"/src/web","code":{"connection":"c-code","repo":"acme/web"},"run":"bun dev"}]}"#;

fn a_space_with_no_repository_and_a_dev_application_saved_before_both_load(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    place.raw("INSERT INTO applications (id, position, data, seq) VALUES ('old', 0, ?1, 1)", args![SAVED_BEFORE_SPACES]);
    let finance = Application {
        id: "fin".into(),
        name: "Financeiro".into(),
        bindings: Bindings::from([("purchasing".into(), Binding::new("c-erp", "ACME"))]),
        instructions: "Pague só com pedido aprovado.".into(),
        skills: vec!["conciliacao".into()],
        workflow: "invoice-match".into(),
        ..Application::default()
    };
    store.save_application(&finance).expect("saved");
    assert_eq!(store.application("fin").expect("read"), Some(finance));
    let old = store.application("old").expect("read").expect("the old application");
    assert_eq!(old.bound("tracker"), Some(&Binding::new("c-tracker", "APP")));
    assert_eq!(old.repositories[0].bound("code"), Some(&Binding::new("c-code", "acme/web")));
    assert_eq!(old.repositories[0].run, ["bun dev"]);
    assert!(old.instructions.is_empty() && old.skills.is_empty() && old.workflow.is_empty());
    store.save_application(&old).expect("saved again");
    let names: Vec<String> = store.applications().expect("listed").into_iter().map(|found| found.name).collect();
    assert_eq!(names, ["Loja", "Financeiro"]);
}

#[test]
fn what_a_space_adds_is_left_out_of_an_application_that_does_not_use_it() {
    let saved = serde_json::to_value(Application { name: "Loja".into(), ..Application::default() }).expect("saved");
    for key in ["instructions", "skills", "workflow"] {
        assert!(saved.get(key).is_none(), "{key}");
    }
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

scenarios!(
    applications_are_kept_in_order_changed_in_place_and_removed,
    a_space_with_no_repository_and_a_dev_application_saved_before_both_load,
);
