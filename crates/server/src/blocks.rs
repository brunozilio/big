use serde_json::{Value, json};
use store::ConnectionRecord;

use crate::marks::serving;

pub const REVIEW: &str = "review";
pub const MERGE: &str = "merge";
pub const SUBMIT: &str = "submit";

pub struct Place<'a> {
    pub connections: &'a [ConnectionRecord],
    pub waiting: bool,
}

impl Place<'_> {
    fn connection(&self, capability: &str) -> String {
        serving(self.connections, capability).map(|record| record.id.clone()).unwrap_or_default()
    }
}

fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Null => String::new(),
        other => other["ref"].as_str().map_or_else(|| other.to_string(), str::to_owned),
    }
}

fn field(raw: &Value) -> Value {
    let name = raw["name"].as_str().or(raw["id"].as_str()).unwrap_or("value");
    let kind = raw["kind"].as_str().filter(|kind| ["text", "textarea", "select"].contains(kind)).unwrap_or("text");
    let mut value = json!({ "name": name, "label": raw["label"].as_str().unwrap_or(name), "kind": kind });
    for extra in ["value", "placeholder", "options", "mono"] {
        if !raw[extra].is_null() {
            value[extra] = raw[extra].clone();
        }
    }
    value
}

fn button(raw: &Value) -> Value {
    let action = raw["action"].as_str().or(raw["id"].as_str()).unwrap_or_default();
    let mut value = json!({ "action": action, "label": raw["label"].as_str().unwrap_or(action) });
    for extra in ["icon", "variant", "payload"] {
        if !raw[extra].is_null() {
            value[extra] = raw[extra].clone();
        }
    }
    value
}

fn one(place: &Place<'_>, raw: &Value) -> Option<Value> {
    let block = match raw["type"].as_str()? {
        "chat" => json!({ "type": "chat", "session": text(&raw["session"]), "composer": raw["composer"] == true }),
        "issue" => {
            let key = text(if raw["key"].is_null() { &raw["ref"] } else { &raw["key"] });
            json!({ "type": "issue", "connection": place.connection("tracker"), "key": key, "comment": true })
        }
        "change" => json!({ "type": "change", "connection": place.connection("code"), "change": text(&raw["pr"]) }),
        "review" => json!({
            "type": "review", "connection": place.connection("code"), "change": text(&raw["pr"]),
            "editable": place.waiting, "session": raw["session"].as_str(), "submit": REVIEW,
        }),
        "pipeline" => json!({
            "type": "pipeline", "connection": place.connection("pipelines"), "run": text(&raw["run"]), "gate": null,
        }),
        "merge" => json!({
            "type": "merge", "connection": place.connection("code"), "change": text(&raw["pr"]),
            "ready": place.waiting, "methods": raw["methods"].as_array().cloned().unwrap_or_default(),
            "action": MERGE, "checks": [], "result": null,
        }),
        "markdown" => json!({ "type": "markdown", "text": text(&raw["text"]), "muted": raw["muted"] == true }),
        "cases" => cases(place, raw),
        "logs" => logs(raw),
        "form" => json!({
            "type": "form",
            "fields": raw["fields"].as_array().map(|fields| fields.iter().map(field).collect::<Vec<_>>()).unwrap_or_default(),
            "submit": { "action": SUBMIT, "label": "Enviar" },
        }),
        "actions" => json!({
            "type": "actions",
            "buttons": raw["buttons"].as_array().map(|all| all.iter().map(button).collect::<Vec<_>>()).unwrap_or_default(),
        }),
        _ => return None,
    };
    Some(block)
}

fn optional(raw: &Value) -> Value {
    match raw {
        Value::String(text) if !text.trim().is_empty() => Value::String(text.clone()),
        _ => Value::Null,
    }
}

fn clauses(raw: &Value) -> Value {
    let lines =
        raw.as_array().map(|all| all.iter().map(text).filter(|line| !line.trim().is_empty()).collect::<Vec<_>>());
    json!(lines.unwrap_or_default())
}

fn case(raw: &Value) -> Value {
    let kind = if raw["kind"] == "ui" { "ui" } else { "api" };
    json!({
        "title": text(&raw["title"]),
        "kind": kind,
        "objective": optional(&raw["objective"]),
        "ref": optional(&raw["ref"]),
        "url": optional(&raw["url"]),
        "given": clauses(&raw["given"]),
        "when": clauses(&raw["when"]),
        "then": clauses(&raw["then"]),
    })
}

fn cases(place: &Place<'_>, raw: &Value) -> Value {
    let all = raw["cases"].as_array().map(|all| all.iter().map(case).collect::<Vec<_>>()).unwrap_or_default();
    let approve = optional(&raw["approve"]);
    let changes = optional(&raw["changes"]);
    let editable = place.waiting && !approve.is_null();
    json!({
        "type": "cases", "cases": all, "session": optional(&raw["session"]),
        "editable": editable, "approve": approve, "changes": changes,
    })
}

fn logs(raw: &Value) -> Value {
    if let Some(commands) = raw["commands"].as_array() {
        return json!({ "type": "logs", "commands": commands });
    }
    let command = json!({ "name": "Saída", "state": "done", "seconds": null, "log": text(&raw["text"]) });
    json!({ "type": "logs", "commands": [command] })
}

pub fn normalized(place: &Place<'_>, raw: &Value) -> Vec<Value> {
    let listed = match raw {
        Value::Array(all) => all.clone(),
        Value::Null => Vec::new(),
        other => vec![other.clone()],
    };
    listed.iter().filter_map(|block| one(place, block)).collect()
}

pub fn input(action: &str, payload: Value) -> (String, Value) {
    if action != REVIEW {
        return (action.to_owned(), payload);
    }
    let verdict = match payload["verdict"].as_str() {
        Some("approve") => "approve",
        Some("changes") => "request_changes",
        _ => "comment",
    };
    let lines: Vec<Value> = payload["notes"]
        .as_array()
        .map(|notes| {
            notes
                .iter()
                .map(|note| json!({ "path": note["file"], "line": note["line"], "body": note["body"] }))
                .collect()
        })
        .unwrap_or_default();
    (verdict.to_owned(), json!({ "body": payload["body"], "lines": lines }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, capability: &str) -> ConnectionRecord {
        ConnectionRecord {
            id: id.into(),
            provider: "p".into(),
            label: id.into(),
            account: None,
            capabilities: vec![capability.into()],
            settings: json!({}),
        }
    }

    #[test]
    fn the_script_s_blocks_take_the_contract_s_shape() {
        let connections = [record("t", "tracker"), record("c", "code")];
        let place = Place { connections: &connections, waiting: true };
        let raw = json!([
            { "type": "chat", "session": "s1" },
            { "type": "issue", "ref": "K-1" },
            { "type": "merge", "pr": "o/r#3", "methods": ["squash"] },
            { "type": "logs", "text": "ok" },
            { "type": "actions", "buttons": [{ "id": "retry", "label": "retry" }] },
            { "type": "unknown" },
        ]);
        let blocks = normalized(&place, &raw);
        assert_eq!(blocks.len(), 5);
        assert_eq!(blocks[0], json!({ "type": "chat", "session": "s1", "composer": false }));
        assert_eq!(blocks[1]["connection"], "t");
        assert_eq!(blocks[2]["ready"], true);
        assert_eq!(blocks[3]["commands"][0]["log"], "ok");
        assert_eq!(blocks[4]["buttons"][0]["action"], "retry");
    }

    #[test]
    fn test_cases_keep_their_structure_for_the_page() {
        let place = Place { connections: &[], waiting: false };
        let raw = json!({ "type": "cases", "cases": [{
            "title": "Criar", "kind": "ui", "objective": " ", "ref": "T-1",
            "given": ["um cliente"], "when": ["envio", ""], "then": ["201", "o corpo traz o id"],
        }] });
        let blocks = normalized(&place, &raw);
        assert_eq!(blocks[0]["editable"], false);
        let first = &blocks[0]["cases"][0];
        assert_eq!(first["kind"], "ui");
        assert_eq!(first["objective"], Value::Null);
        assert_eq!(first["ref"], "T-1");
        assert_eq!(first["when"], json!(["envio"]));
        assert_eq!(first["then"], json!(["201", "o corpo traz o id"]));
    }

    #[test]
    fn a_review_from_the_page_becomes_the_step_s_verdict() {
        let payload =
            json!({ "verdict": "changes", "body": "x", "notes": [{ "file": "a.rs", "line": 2, "body": "y" }] });
        let (action, values) = input(REVIEW, payload);
        assert_eq!(action, "request_changes");
        assert_eq!(values["lines"][0], json!({ "path": "a.rs", "line": 2, "body": "y" }));
        assert_eq!(input("merge", json!({ "method": "squash" })).0, "merge");
    }
}
