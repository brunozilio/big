use domain::PromptRequest;
use serde_json::Value;
use store::{Application, Store};

use crate::protocol::Command;

fn of(store: &Store, input: &Value) -> Option<Application> {
    let id = crate::application::id_of(input)?;
    store.application(&id).ok().flatten()
}

pub fn append(request: &mut PromptRequest, text: String) {
    let system = request.session.append_system.take().unwrap_or_default();
    request.session.append_system = Some(if system.is_empty() { text } else { format!("{system}\n\n{text}") });
}

pub fn brief(store: &Store, input: &Value, request: &mut PromptRequest) {
    let Some(space) = of(store, input) else { return };
    let instructions = space.instructions.trim();
    if !instructions.is_empty() {
        append(request, format!("# Instructions for the work in {}\n{instructions}", space.name));
    }
    for skill in space.skills.iter().map(|skill| skill.trim()).filter(|skill| !skill.is_empty()) {
        if !request.session.skills.iter().any(|known| known == skill) {
            request.session.skills.push(skill.to_owned());
        }
    }
}

fn bound(store: &Store, input: &Value, capability: &str) -> Option<String> {
    let live = of(store, input).and_then(|space| space.bound(capability).map(|binding| binding.connection.clone()));
    let kept = || input["application"]["bindings"][capability]["connection"].as_str().map(str::to_owned);
    live.or_else(kept).filter(|connection| !connection.is_empty())
}

pub fn routed(store: &Store, owner: &str, input: &Value, command: &Command) -> Command {
    let mut command = command.clone();
    if command.connection.is_some() {
        return command;
    }
    let capability = command.call().capability;
    let Some(connection) = bound(store, input, &capability) else { return command };
    let offers = |record: &store::ConnectionRecord| {
        record.id == connection && record.capabilities.iter().any(|name| name == &capability)
    };
    if store.connections_of(owner).is_ok_and(|records| records.iter().any(offers)) {
        command.connection = Some(connection);
    }
    command
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use store::{Application, Binding, Bindings, ConnectionRecord, Store};

    use super::routed;
    use crate::protocol::Command;

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

    fn call(connection: Option<&str>) -> Command {
        let command = json!({ "seq": 1, "kind": "call", "branch": "main", "connection": connection,
            "args": { "capability": "purchasing", "action": "invoice", "args": {} } });
        serde_json::from_value(command).expect("command")
    }

    fn routed_to(store: &Store, bound: &str, given: Option<&str>) -> Option<String> {
        let space = Application {
            id: "fin".into(),
            name: "Financeiro".into(),
            bindings: Bindings::from([("purchasing".into(), Binding::new(bound, "ACME"))]),
            ..Application::default()
        };
        store.save_application(&space).expect("saved");
        let input = json!({ "application": { "id": "fin", "name": "Financeiro" } });
        routed(store, domain::principal::LOCAL_USER, &input, &call(given)).connection
    }

    #[test]
    fn a_call_without_a_connection_goes_to_the_one_the_space_binds_for_that_capability() {
        let store = Store::in_memory(0).expect("store");
        for (id, capability) in [("erp-a", "purchasing"), ("erp-b", "purchasing"), ("jira", "tracker")] {
            store.save_connection(&record(id, capability)).expect("connection");
        }
        assert_eq!(routed_to(&store, "erp-b", None).as_deref(), Some("erp-b"));
        assert_eq!(routed_to(&store, "erp-b", Some("erp-a")).as_deref(), Some("erp-a"), "the workflow's choice wins");
        assert_eq!(routed_to(&store, "jira", None), None, "a connection without the capability is not used");
        assert_eq!(routed_to(&store, "gone", None), None, "nor one that no longer exists");
    }
}
