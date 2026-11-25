use serde_json::{Value, json};

use super::probe;

const OPERATIONS: &str = "connectors/jira/operations.ts";

fn discovered(value: Value, topic: &str, tried: &str) -> Value {
    let body = "return m.discovered(input.value, input.topic, input.tried) ?? null;";
    probe(OPERATIONS, body, json!({ "value": value, "topic": topic, "tried": tried }))
}

#[test]
fn an_operation_is_picked_from_what_discover_lists() {
    let text =
        json!("- getJiraProject: one project\n- listJiraProjects: projects visible to you\n  inputs: issueIdOrKey");
    assert_eq!(discovered(text.clone(), "project", "x"), json!("listJiraProjects"));
    assert_eq!(discovered(text, "project", "listJiraProjects"), json!("getJiraProject"));
    assert_eq!(discovered(json!("transitionId is required"), "transition", "x"), Value::Null);
    let nested = json!({"data": {"projects": [{"key": "A"}], "isLast": true}});
    let found = probe(OPERATIONS, "return m.foundList(input, [\"values\", \"projects\"]);", nested.clone());
    assert_eq!(found, json!([{"key": "A"}]));
    assert_eq!(probe(OPERATIONS, "return m.lastPage(input);", nested), json!(true));
}

#[test]
fn a_jql_text_keeps_only_letters_digits_and_single_spaces_and_a_key_is_checked() {
    let api = "connectors/jira/api.ts";
    assert_eq!(probe(api, "return m.jqlText(input);", json!("  login!  falha\tção ")), json!("login falha ção"));
    assert_eq!(probe(api, "return m.jqlKey(input);", json!("BIG-1")), json!("BIG-1"));
    let refused = probe(api, "try { m.jqlKey(input); return null; } catch (e) { return e.message; }", json!("a b"));
    assert_eq!(refused, json!("\"a b\" is not a project or issue key"));
}
