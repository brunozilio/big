use std::path::PathBuf;

use super::*;

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

fn moved(from: &str, to: &str) -> Retired {
    Retired { from: from.to_owned(), to: Successor::By(to.to_owned()) }
}

fn checked(current: &[&str], ledger: &[&str], before: &[&str], gone: &[Retired]) -> Vec<String> {
    let (current, ledger, before) = (set(current), set(ledger), set(before));
    check(&Ledgers { current: &current, ledger: &ledger, before: &before, retired: gone })
}

#[test]
fn removing_a_published_id_fails() {
    let found = checked(&["action:x/a.get"], &["action:x/a.get", "action:x/a.old"], &[], &[]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("published-id action:x/a.old: removed without retirement"), "{found:?}");
}

#[test]
fn adding_then_migrating_then_retiring_passes() {
    let new = checked(&["action:x/a.old", "action:x/a.new"], &["action:x/a.old"], &["action:x/a.old"], &[]);
    assert_eq!(new, ["published-id action:x/a.new: new id, add it with `xtask conformance --publish`"]);
    let both = ["action:x/a.old", "action:x/a.new"];
    assert_eq!(checked(&both, &both, &["action:x/a.old"], &[]), Vec::<String>::new());
    let retired = [moved("action:x/a.old", "action:x/a.new")];
    assert_eq!(checked(&["action:x/a.new"], &both, &both, &retired), Vec::<String>::new());
}

#[test]
fn a_retired_id_cannot_come_back_nor_point_to_nothing() {
    let retired = [moved("route:/b", "route:/a")];
    let found = checked(&["route:/a", "route:/b"], &["route:/a", "route:/b"], &[], &retired);
    assert_eq!(found, ["published-id route:/b: retired, so it cannot be published again"]);
    let found = checked(&["route:/a"], &["route:/a", "route:/b"], &[], &[moved("route:/b", "route:/c")]);
    assert_eq!(found, ["published-id route:/b: retired in favor of route:/c, which is not published"]);
    let removed = [Retired { from: "route:/b".into(), to: Successor::Removed { reason: "gone".into() } }];
    assert_eq!(checked(&["route:/a"], &["route:/a", "route:/b"], &[], &removed), Vec::<String>::new());
}

#[test]
fn an_id_cannot_leave_the_ledger_even_when_retired() {
    let retired = [moved("route:/b", "route:/a")];
    let found = checked(&["route:/a"], &["route:/a"], &["route:/a", "route:/b"], &retired);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("published-id route:/b: left content/published.json"), "{found:?}");
}

#[test]
fn the_tables_read_the_ids_and_refuse_malformed_entries() {
    assert_eq!(ids_of(&ledger_text(&set(&["b", "a"]))), Ok(set(&["a", "b"])));
    assert!(ids_of("{ \"ids\": [\"a\", 3] }").is_err());
    assert!(ids_of("{ \"schema_version\": 1 }").is_err());
    let table = r#"{ "imports": [], "ids": [{ "from": "route:/old", "to": "route:/new" }, { "from": "route:/gone", "to": null, "reason": "no one calls it" }] }"#;
    let removed = Retired { from: "route:/gone".into(), to: Successor::Removed { reason: "no one calls it".into() } };
    assert_eq!(retired_of(table), Ok(vec![moved("route:/old", "route:/new"), removed]));
    assert_eq!(retired_of("{ \"imports\": [] }"), Ok(Vec::new()));
    for broken in [
        r#"{ "ids": [{ "from": "route:/gone" }] }"#,
        r#"{ "ids": [{ "from": "route:/gone", "to": null }] }"#,
        r#"{ "ids": [{ "to": "route:/new" }] }"#,
        r#"{ "ids": [{ "from": "route:/gone", "to": 3 }] }"#,
        r#"{ "ids": {} }"#,
    ] {
        assert!(retired_of(broken).is_err(), "{broken}");
    }
}

#[test]
fn every_shipped_server_route_is_in_the_ledger() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let ledger = ids_of(&std::fs::read_to_string(root.join(LEDGER)).expect("ledger")).expect("ids");
    let routes = crate::routes::routes(&root).expect("routes");
    let missing: Vec<&String> = routes.difference(&ledger).collect();
    assert!(missing.is_empty(), "{missing:?}");
    let retired = retired_of(&std::fs::read_to_string(root.join(RETIRED)).expect("retired")).expect("retired ids");
    assert!(retired.iter().all(|entry| !routes.contains(&entry.from)), "{retired:?}");
}
