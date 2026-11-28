use super::*;

fn store() -> Store {
    Store::in_memory(0).expect("store")
}

fn denied_push() -> Allowed {
    Allowed::of([Rule::new(Grant::Command("git push".into()), Behavior::Deny)])
}

#[test]
fn a_broken_list_keeps_the_last_good_denials_and_says_why() {
    let store = store();
    saved(&store, &denied_push()).expect("saved");
    store.set_preference(KEY, &json!({ "grants": "oops" })).expect("broken");
    let kept = loaded(&store);
    assert_eq!(kept.allowed, denied_push());
    let problem = kept.problem.expect("a problem");
    assert_eq!(problem.fallback, Some(Fallback::LastGood));
    assert!(problem.error.as_deref().is_some_and(|error| error.contains("schema_version")), "{problem:?}");
    assert!(seeded(&store).problem.is_some(), "the defaults do not cover a broken list");
}

#[test]
fn a_broken_list_with_no_good_copy_closes_commands_and_sites() {
    let store = store();
    store.set_preference(KEY, &json!({ "schema_version": 1, "grants": [{ "kind": 5 }] })).expect("broken");
    let kept = loaded(&store);
    assert_eq!(kept.problem.and_then(|problem| problem.fallback), Some(Fallback::Closed));
    let effects = [domain::permission::Effect::exec("ls")];
    let call = domain::permission::Call { tool: "Bash", subject: "ls", effects: &effects };
    assert_eq!(kept.allowed.verdict(&call), Some(Behavior::Deny));
}

#[test]
fn an_old_plain_list_still_loads_and_an_unknown_version_does_not() {
    let store = store();
    store.set_preference(KEY, &json!([{ "kind": "site", "value": "gov.br" }])).expect("old");
    assert!(kept(&store).has(&Grant::Site("gov.br".into())));
    assert_eq!(loaded(&store).problem, None);
    store.set_preference(KEY, &json!({ "schema_version": 9, "grants": [] })).expect("newer");
    assert!(loaded(&store).problem.is_some());
}

#[test]
fn a_saved_grant_for_everything_is_left_out_and_named_in_the_problem() {
    let store = store();
    let saved_list = json!({ "schema_version": 1, "grants": [
        { "kind": "command", "value": "*" },
        { "kind": "site", "value": "*", "behavior": "ask" },
        { "kind": "site", "value": "*", "behavior": "deny" },
        { "kind": "command", "value": "ls" },
    ] });
    store.set_preference(KEY, &saved_list).expect("saved by hand");
    let kept = seeded(&store);
    assert_eq!(
        kept.allowed.rules(),
        [Rule::new(Grant::Site("*".into()), Behavior::Deny), Rule::from(Grant::Command("ls".into()))]
    );
    let problem = kept.problem.expect("told");
    assert_eq!(problem.fallback, None);
    assert_eq!(
        problem.dropped,
        [Rule::from(Grant::Command("*".into())), Rule::new(Grant::Site("*".into()), Behavior::Ask)]
    );
    let shown = serde_json::to_value(&problem).expect("json");
    assert_eq!(
        shown,
        json!({ "dropped": [
        { "kind": "command", "value": "*", "behavior": "allow" },
        { "kind": "site", "value": "*", "behavior": "ask" },
    ] })
    );
    assert_eq!(store.preference(KEY).expect("read"), Some(saved_list), "kept as it was until the user saves");
}

#[test]
fn nothing_is_written_while_the_list_is_closed() {
    let store = store();
    store.set_preference(KEY, &json!("broken")).expect("broken");
    let closed = loaded(&store).problem.expect("closed");
    assert!(!persisted(&store, Some(&closed), &denied_push()));
    assert_eq!(store.preference(KEY).expect("read"), Some(json!("broken")), "the broken copy stays for repair");
    let last_good = Problem { fallback: Some(Fallback::LastGood), ..Problem::default() };
    assert!(persisted(&store, Some(&last_good), &denied_push()));
    assert_eq!(kept(&store), denied_push());
}
