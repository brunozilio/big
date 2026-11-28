use domain::permission::Behavior;

use super::store::{Fallback, Problem};

use super::*;

#[test]
fn the_list_is_tidied_and_kept_in_the_store() {
    let store = Store::in_memory(0).expect("store");
    let body = json!({ "grants": [
        { "kind": "command", "value": "  cargo   test " },
        { "kind": "site", "value": "https://www.gov.br/pt-br" },
        { "kind": "site", "value": "www.gov.br" },
        { "kind": "tool", "value": "" },
        { "kind": "command", "value": "git push", "behavior": "deny" },
    ] });
    let saved = replaced(&store, &body).expect("saved");
    assert_eq!(
        saved.rules(),
        [
            Rule::from(Grant::Command("cargo test".into())),
            Rule::from(Grant::Site("www.gov.br".into())),
            Rule::new(Grant::Command("git push".into()), Behavior::Deny),
        ]
    );
    assert_eq!(kept(&store), saved);
    assert!(replaced(&store, &json!({ "grants": [{ "kind": "nope", "value": "x" }] })).is_err());
}

#[test]
fn the_defaults_come_once_and_keep_what_was_saved() {
    let store = Store::in_memory(0).expect("store");
    replaced(&store, &json!({ "grants": [{ "kind": "site", "value": "gov.br" }] })).expect("saved");
    let first = seeded(&store).allowed;
    assert!(first.has(&Grant::Site("gov.br".into())));
    assert!(first.has(&Grant::Command("git status".into())));
    replaced(&store, &json!({ "grants": [] })).expect("emptied");
    assert!(seeded(&store).allowed.rules().is_empty());
}

#[test]
fn a_grant_for_everything_is_refused() {
    let store = Store::in_memory(0).expect("store");
    for (kind, value) in [("site", "*"), ("command", "*"), ("site", "*.com")] {
        let body = json!({ "grants": [{ "kind": kind, "value": value }] });
        assert!(replaced(&store, &body).is_err(), "{kind} {value}");
    }
    let denied = json!({ "grants": [{ "kind": "site", "value": "*", "behavior": "deny" }] });
    assert_eq!(replaced(&store, &denied).expect("a denial for everything is fine").rules().len(), 1);
    assert!(replaced(&store, &json!({ "grants": [{ "kind": "site", "value": "*.gov.br" }] })).is_ok());
}

#[test]
fn the_list_shows_its_problem_until_a_save_clears_it() {
    let store = Store::in_memory(0).expect("store");
    let dropped = vec![Rule::from(Grant::Command("*".into()))];
    set_problem(Some(Problem { fallback: Some(Fallback::LastGood), error: Some("oops".into()), dropped }));
    let shown = listed(&Allowed::default());
    assert_eq!(
        shown["problem"],
        json!({ "fallback": "last_good", "error": "oops", "dropped": [{ "kind": "command", "value": "*", "behavior": "allow" }] })
    );
    assert!(replaced(&store, &json!({ "grants": [{ "kind": "command", "value": "*" }] })).is_err());
    assert!(problem().is_some(), "a refused save leaves the problem in view");
    replaced(&store, &json!({ "grants": [{ "kind": "command", "value": "ls" }] })).expect("saved");
    assert_eq!(problem(), None);
    assert_eq!(listed(&kept(&store))["problem"], Value::Null);
}
