use domain::workflow::{WorkItem, WorkflowEvent};
use serde_json::{Value, json};

const ITEM_LANE: &str = "branch";
const BASE: &str = "base";

pub(crate) fn event(text: &str) -> serde_json::Result<Option<WorkflowEvent>> {
    let value: Value = serde_json::from_str(text)?;
    let data = &value["data"];
    let upgraded = match value["type"].as_str() {
        Some("branched") => Some(bound("", ITEM_LANE, &data["name"], true)),
        Some("base_chosen") => Some(bound(data["repository"].as_str().unwrap_or_default(), BASE, &data["base"], false)),
        _ => None,
    };
    WorkflowEvent::decoded(upgraded.map_or(value, |data| json!({ "type": "bound", "data": data })))
}

fn bound(connection: &str, kind: &str, value: &Value, keep: bool) -> Value {
    json!({ "binding": { "connection": connection, "kind": kind, "value": value }, "keep": keep })
}

pub(crate) fn item(text: &str) -> serde_json::Result<WorkItem> {
    let mut value: Value = serde_json::from_str(text)?;
    if let Some(fields) = value.as_object_mut()
        && let Some(lane) = fields.remove(ITEM_LANE).filter(Value::is_string)
    {
        let bindings = fields.entry("bindings").or_insert_with(|| json!([]));
        let known = bindings
            .as_array()
            .is_some_and(|all| all.iter().any(|binding| binding["connection"] == "" && binding["kind"] == ITEM_LANE));
        if let (Some(all), false) = (bindings.as_array_mut(), known) {
            all.push(json!({ "connection": "", "kind": ITEM_LANE, "value": lane }));
        }
    }
    serde_json::from_value(value)
}

#[cfg(test)]
mod tests {
    use domain::workflow::Binding;

    use super::*;

    #[test]
    fn a_retired_lane_event_becomes_a_kept_item_binding() {
        let read = event(r#"{"type":"branched","data":{"name":"big/x"}}"#).expect("read");
        assert_eq!(
            read,
            Some(WorkflowEvent::Bound { binding: Binding::new("", ITEM_LANE, json!("big/x")), keep: true })
        );
    }

    #[test]
    fn a_retired_chosen_base_becomes_a_binding_of_its_repository() {
        let read = event(r#"{"type":"base_chosen","data":{"repository":"web","base":"main"}}"#).expect("read");
        assert_eq!(read, Some(WorkflowEvent::Bound { binding: Binding::new("web", BASE, json!("main")), keep: false }));
    }

    #[test]
    fn current_events_read_as_they_are() {
        let named = serde_json::to_string(&WorkflowEvent::Named { title: "t".into() }).expect("named");
        assert_eq!(event(&named).expect("read"), Some(WorkflowEvent::Named { title: "t".into() }));
        assert_eq!(event(r#"{"type":"heads_up","data":{}}"#).expect("retired"), None);
        let bound = WorkflowEvent::Bound { binding: Binding::new("c", "k", json!({ "n": 1 })), keep: false };
        assert_eq!(event(&serde_json::to_string(&bound).expect("bound")).expect("read"), Some(bound));
    }

    #[test]
    fn an_item_saved_with_its_lane_loads_it_as_a_binding() {
        let mut saved = serde_json::to_value(WorkItem::default()).expect("item");
        saved[ITEM_LANE] = json!("big/x");
        let loaded = item(&saved.to_string()).expect("loaded");
        assert_eq!(loaded.bound("", ITEM_LANE).and_then(Binding::text), Some("big/x"));
        saved[ITEM_LANE] = Value::Null;
        assert!(item(&saved.to_string()).expect("no lane").bindings.is_empty());
    }
}
