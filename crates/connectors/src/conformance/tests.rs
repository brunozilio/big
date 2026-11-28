use serde_json::{Value, json};

use super::*;

const LISTS: &str = r#"import { fetchJson, text, unknownAction } from "sdk/connectors/index.ts";

export function provider() {
  return {
    schema_version: 2,
    name: "Things",
    capabilities: ["things"],
    auth: { kind: "token", fields: [], form: "bearer" },
    actions: [
      { name: "things.list", writes: false, effects: ["read"], risk: "low", reversible: true, untrusted: false, sensitive: false,
        input_schema: { type: "object" }, output_schema: { type: "object", properties: { items: { type: "array" } } } },
      { name: "things.add", writes: true, effects: EFFECTS, risk: "low", reversible: true, untrusted: false, sensitive: false,
        input_schema: { type: "object", required: ["title"] }, output_schema: { type: "object" } },
    ],
  };
}

export function call({ action, args }, ctx) {
  if (action === "things.list") {
    const page = fetchJson(ctx, { url: `https://things.example/list?after=${args.after ?? ""}` });
    return { items: page.items, next: page.next ?? "" };
  }
  if (action === "things.add") return fetchJson(ctx, { method: "POST", url: "https://things.example/add", body: text(args, "title") });
  return unknownAction(action);
}

export function accounts() {
  return [{ id: "a", label: "A", settings: {} }];
}
"#;

fn root(effects: &str, fixture: Option<Value>) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let folder = root.path().join("connectors/things");
    std::fs::create_dir_all(&folder).unwrap_or_else(|error| panic!("{error}"));
    let source = LISTS.replace("EFFECTS", effects);
    std::fs::write(folder.join("index.ts"), source).unwrap_or_else(|error| panic!("{error}"));
    if let Some(fixture) = fixture {
        std::fs::write(folder.join(FILE), fixture.to_string()).unwrap_or_else(|error| panic!("{error}"));
    }
    root
}

fn passing() -> Value {
    json!({
        "schema_version": 1,
        "cases": [
            { "name": "lists two pages", "action": "things.list",
              "http": [{ "method": "GET", "url": "/list?after=", "body": { "items": [1], "next": "p2" } }],
              "expect": { "ok": { "items": [1], "next": "p2" } },
              "next": { "cursor": "/next", "arg": "after",
                        "http": [{ "method": "GET", "url": "/list?after=p2", "body": { "items": [2] } }],
                        "expect": { "ok": { "items": [2] } } } },
            { "name": "adds", "action": "things.add", "args": { "title": "x" },
              "http": [{ "method": "POST", "url": "/add", "body": { "id": "t1" } }],
              "expect": { "ok": { "id": "t1" } }, "sent": ["POST https://things.example/add"] },
            { "name": "a missing title is refused", "action": "things.add", "expect": { "error": "permanent" } },
            { "name": "accounts", "action": "accounts", "expect": { "ok": [{ "id": "a" }] } }
        ]
    })
}

fn findings(root: &tempfile::TempDir) -> Vec<String> {
    run(root.path()).findings
}

#[test]
fn a_connector_that_follows_its_fixtures_passes_and_publishes_its_ids() {
    let root = root("[\"write\"]", Some(passing()));
    let report = run(root.path());
    assert_eq!(report.findings, Vec::<String>::new());
    let ids: Vec<&str> = report.ids.iter().map(String::as_str).collect();
    assert_eq!(ids, ["action:things/things.add", "action:things/things.list", "connector:things"]);
}

#[test]
fn a_connector_whose_write_declares_no_effects_fails() {
    let found = findings(&root("[]", Some(passing())));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("conformance connectors/things: does not load:"), "{found:?}");
    assert!(found[0].contains("things.add writes but declares no effects"), "{found:?}");
}

#[test]
fn a_connector_without_fixtures_or_with_an_uncovered_action_fails() {
    let found = findings(&root("[\"write\"]", None));
    assert_eq!(
        found,
        ["conformance connectors/things: declares no conformance.json: every connector ships its fixtures"]
    );
    let mut partial = passing();
    partial["cases"] = json!([partial["cases"][0].clone()]);
    let found = findings(&root("[\"write\"]", Some(partial)));
    assert_eq!(found, ["conformance connectors/things: things.add has no case in conformance.json"]);
}

#[test]
fn wrong_answers_wrong_classes_and_unmocked_requests_are_findings() {
    let mut fixture = passing();
    fixture["cases"][1]["expect"] = json!({ "ok": { "id": "other" } });
    fixture["cases"][2]["expect"] = json!({ "error": "auth" });
    fixture["cases"][0]["next"]["http"] = json!([]);
    let found = findings(&root("[\"write\"]", Some(fixture)));
    let has = |part: &str| found.iter().any(|finding| finding.contains(part));
    assert!(has("adds: answered {\"id\":\"t1\"}, expected it to contain {\"id\":\"other\"}"), "{found:?}");
    assert!(has("a missing title is refused: failed as permanent, expected auth"), "{found:?}");
    assert!(
        has("lists two pages: sent a request no mock answers: GET https://things.example/list?after=p2"),
        "{found:?}"
    );
}

#[test]
fn a_last_page_that_still_carries_a_cursor_is_a_finding() {
    let mut fixture = passing();
    fixture["cases"][0]["next"]["http"][0]["body"] = json!({ "items": [2], "next": "p3" });
    let found = findings(&root("[\"write\"]", Some(fixture)));
    assert_eq!(
        found,
        ["conformance connectors/things: lists two pages: the last page still carries a cursor at /next"]
    );
}

#[test]
fn a_connector_that_hides_a_failed_answer_breaks_the_typed_errors() {
    let swallowing = LISTS.replace("EFFECTS", "[\"write\"]").replace(
        "return fetchJson(ctx, { method: \"POST\"",
        "try { return fetchJson(ctx, { method: \"POST\", url: \"https://things.example/add\" }); } catch (_) { return { id: \"t1\" }; }\n  return fetchJson(ctx, { method: \"POST\"",
    );
    let root = root("[\"write\"]", Some(passing()));
    std::fs::write(root.path().join("connectors/things/index.ts"), swallowing)
        .unwrap_or_else(|error| panic!("{error}"));
    let found = findings(&root);
    assert!(
        found.iter().any(|finding| finding.contains("adds: with every answer at 401 it still answered")),
        "{found:?}"
    );
    assert!(
        found.iter().any(|finding| finding.contains("adds: with every answer at 503 it still answered")),
        "{found:?}"
    );
}

#[test]
fn an_action_that_declares_the_statuses_it_handles_skips_only_those_replays() {
    let swallowing = LISTS.replace("EFFECTS", "[\"write\"]").replace(
        "return fetchJson(ctx, { method: \"POST\"",
        "try { return fetchJson(ctx, { method: \"POST\", url: \"https://things.example/add\" }); } catch (_) { return { id: \"t1\" }; }\n  return fetchJson(ctx, { method: \"POST\"",
    );
    let mut fixture = passing();
    fixture["handles"] = json!({ "things.add": [401] });
    let handled = root("[\"write\"]", Some(fixture));
    std::fs::write(handled.path().join("connectors/things/index.ts"), swallowing)
        .unwrap_or_else(|error| panic!("{error}"));
    let found = findings(&handled);
    assert!(found.iter().any(|finding| finding.contains("adds: with every answer at 503")), "{found:?}");
    assert!(!found.iter().any(|finding| finding.contains("at 401")), "{found:?}");
    let mut unknown = passing();
    unknown["handles"] = json!({ "things.remove": [404] });
    let found = findings(&root("[\"write\"]", Some(unknown)));
    assert_eq!(found, ["conformance connectors/things: handles names things.remove, which the connector does not offer"]);
}
