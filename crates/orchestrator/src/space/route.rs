use serde_json::Value;
use store::{ConnectionRecord, Store};

pub fn bound(store: &Store, input: &Value, capability: &str) -> Option<String> {
    let connection = match super::live(store, input) {
        Some(Some(space)) => space.bound(capability).map(|binding| binding.connection.clone()),
        Some(None) => input["application"]["bindings"][capability]["connection"].as_str().map(str::to_owned),
        None => None,
    };
    connection.filter(|connection| !connection.is_empty())
}

fn offers(record: &ConnectionRecord, capability: &str) -> bool {
    record.capabilities.iter().any(|name| name == capability)
}

pub fn chosen(
    records: Vec<ConnectionRecord>,
    connection: Option<&str>,
    bound: Option<&str>,
    capability: &str,
) -> Option<ConnectionRecord> {
    if let Some(id) = connection {
        return records.into_iter().find(|record| record.id == id);
    }
    let held = bound.and_then(|id| records.iter().position(|record| record.id == id && offers(record, capability)));
    let at = held.or_else(|| records.iter().position(|record| offers(record, capability)))?;
    records.into_iter().nth(at)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use store::{Application, Binding, Bindings, ConnectionRecord, Store};

    use super::{bound, chosen};

    fn record(id: &str, capability: &str) -> ConnectionRecord {
        ConnectionRecord {
            id: id.into(),
            provider: "fake".into(),
            label: id.into(),
            account: None,
            capabilities: vec![capability.into()],
            settings: json!({}),
        }
    }

    fn records() -> Vec<ConnectionRecord> {
        vec![record("erp-a", "purchasing"), record("erp-b", "purchasing"), record("jira", "tracker")]
    }

    fn picked(connection: Option<&str>, bound: Option<&str>) -> Option<String> {
        chosen(records(), connection, bound, "purchasing").map(|record| record.id)
    }

    #[test]
    fn a_call_without_a_connection_goes_to_the_one_the_space_binds_for_that_capability() {
        assert_eq!(picked(None, Some("erp-b")).as_deref(), Some("erp-b"));
        assert_eq!(picked(Some("erp-a"), Some("erp-b")).as_deref(), Some("erp-a"), "the workflow's choice wins");
        assert_eq!(
            picked(None, Some("jira")).as_deref(),
            Some("erp-a"),
            "a connection without the capability is not used"
        );
        assert_eq!(picked(None, Some("gone")).as_deref(), Some("erp-a"), "nor one that no longer exists");
        assert_eq!(picked(None, None).as_deref(), Some("erp-a"), "unbound, the first one as before");
    }

    fn space(bindings: Bindings) -> Application {
        Application { id: "fin".into(), name: "Financeiro".into(), bindings, ..Application::default() }
    }

    #[test]
    fn the_binding_comes_from_the_live_space_and_from_the_snapshot_only_when_the_space_is_gone() {
        let store = Store::in_memory(0).expect("store");
        let snapshot =
            json!({ "application": { "id": "fin", "bindings": { "purchasing": { "connection": "erp-a" } } } });
        assert_eq!(bound(&store, &snapshot, "purchasing").as_deref(), Some("erp-a"), "the space is gone");
        store.save_application(&space(Bindings::new())).expect("saved");
        assert_eq!(bound(&store, &snapshot, "purchasing"), None, "a binding removed from the space no longer counts");
        let bindings = Bindings::from([("purchasing".to_owned(), Binding::new("erp-b", "ACME"))]);
        store.save_application(&space(bindings)).expect("saved");
        assert_eq!(bound(&store, &snapshot, "purchasing").as_deref(), Some("erp-b"));
        assert_eq!(bound(&store, &json!({ "title": "sem espaço" }), "purchasing"), None);
    }
}
