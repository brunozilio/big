use super::*;

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

fn retired(from: &str, to: &str) -> Retired {
    Retired { from: from.to_owned(), to: to.to_owned() }
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
    let moved = [retired("action:x/a.old", "action:x/a.new")];
    assert_eq!(checked(&both, &both, &both, &moved), Vec::<String>::new());
    assert_eq!(checked(&["action:x/a.new"], &both, &both, &moved), Vec::<String>::new());
}

#[test]
fn an_id_cannot_leave_the_ledger_nor_retire_into_nothing() {
    let found = checked(&["route:/a"], &["route:/a"], &["route:/a", "route:/b"], &[]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("published-id route:/b: left content/published.json"), "{found:?}");
    let found = checked(&["route:/a"], &["route:/a", "route:/b"], &[], &[retired("route:/b", "route:/c")]);
    assert_eq!(found, ["published-id route:/b: retired in favor of route:/c, which is not published"]);
    assert_eq!(
        checked(&["route:/a"], &["route:/a", "route:/b"], &[], &[retired("route:/b", "")]),
        Vec::<String>::new()
    );
}

#[test]
fn literal_routes_are_read_across_lines_and_constants_are_skipped() {
    let source = "Router::new()\n  .route(\"/api/items\", get(list))\n  .route(\n    \"/api/items/{id}\",\n    get(one))\n  .route(ROUTE, get(x))";
    assert_eq!(routes_in(source), ["route:/api/items", "route:/api/items/{id}"]);
}

#[test]
fn the_tables_read_the_ids_and_the_retired_ids() {
    let ledger = ledger_text(&set(&["b", "a"]));
    assert_eq!(ids_of(&ledger), Ok(set(&["a", "b"])));
    let table = r#"{ "imports": [{ "from": "x.ts", "to": "y.ts" }], "ids": [{ "from": "route:/old", "to": "route:/new" }, { "from": "route:/gone" }] }"#;
    assert_eq!(retired_of(table), Ok(vec![retired("route:/old", "route:/new"), retired("route:/gone", "")]));
}

#[test]
fn every_shipped_server_route_is_in_the_ledger() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let ledger = ids_of(&std::fs::read_to_string(root.join(LEDGER)).expect("ledger")).expect("ids");
    let routes = routes(&root).expect("routes");
    assert!(routes.contains("route:/api/items"), "{routes:?}");
    let missing: Vec<&String> = routes.difference(&ledger).collect();
    assert!(missing.is_empty(), "{missing:?}");
}
