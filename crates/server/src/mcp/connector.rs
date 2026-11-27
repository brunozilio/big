use std::sync::Arc;

use connectors::{ActionSpec, Connectors};
use domain::connection::{Capability, ProviderKey};
use domain::mcp::{LocalMcp, McpEndpoint, respond, slug, text_result};
use serde_json::{Value, json};
use store::ConnectionRecord;

use crate::calls::connection;
use crate::state::Ctx;

const ANY: &str = "*";
const NAME_LIMIT: usize = 64;

pub struct Wanted<'a> {
    pub provider: &'a str,
    pub actions: &'a [String],
    pub writes: bool,
}

impl Wanted<'_> {
    fn takes(&self, record: &ConnectionRecord, action: &ActionSpec) -> bool {
        let named = |given: &String| action.name == *given || action.name.starts_with(&format!("{given}."));
        let offered =
            Capability::of_action(&action.name).is_some_and(|capability| connection(record).offers(&capability));
        offered && (self.writes || !action.writes) && (self.actions.is_empty() || self.actions.iter().any(named))
    }
}

struct Tool {
    definition: Value,
    record: ConnectionRecord,
    action: String,
}

pub struct ConnectorTools {
    hub: Arc<Connectors>,
    runtime: tokio::runtime::Handle,
    tools: Vec<Tool>,
}

fn tool_name(label: &str, action: &str) -> String {
    let name = format!("{}_{}", slug(label), action.replace('.', "_"));
    name.chars().take(NAME_LIMIT).collect()
}

fn definition(name: &str, provider: &str, record: &ConnectionRecord, action: &ActionSpec) -> Value {
    let changes = if action.writes { " It changes data there." } else { "" };
    let schema =
        if action.input_schema.is_object() { action.input_schema.clone() } else { json!({ "type": "object" }) };
    json!({
        "name": name,
        "description": format!("Run `{}` on the {provider} connection \"{}\".{changes}", action.name, record.label),
        "inputSchema": schema,
    })
}

fn tools_of(hub: &Connectors, records: Vec<ConnectionRecord>, wanted: &Wanted<'_>) -> Vec<Tool> {
    let mut tools = Vec::new();
    for record in records {
        if wanted.provider != ANY && record.provider != wanted.provider {
            continue;
        }
        let Ok(descriptor) = hub.descriptor(&ProviderKey::from(record.provider.as_str())) else { continue };
        for action in descriptor.actions.iter().filter(|action| wanted.takes(&record, action)) {
            let name = tool_name(&record.label, &action.name);
            let definition = definition(&name, &descriptor.name, &record, action);
            tools.push(Tool { definition, record: record.clone(), action: action.name.clone() });
        }
    }
    tools
}

pub fn local(ctx: &Ctx, wanted: &Wanted<'_>, runtime: tokio::runtime::Handle) -> Option<LocalMcp> {
    let records = ctx.backend.store.connections_of(domain::principal::LOCAL_USER).unwrap_or_default();
    let hub = Arc::clone(&ctx.backend.connectors);
    let tools = tools_of(&hub, records, wanted);
    (!tools.is_empty()).then(|| LocalMcp::new(server(), Arc::new(ConnectorTools { hub, runtime, tools })))
}

fn server() -> String {
    format!("{}_connectors", brand::COMMAND.replace('-', "_"))
}

impl ConnectorTools {
    fn call(&self, tool: &Tool, arguments: &Value) -> Value {
        let Ok(key) = crate::attachments::random_id() else {
            return text_result("No idempotency key could be made.", true);
        };
        let key = format!("{}:{key}", brand::connectors::MARKER);
        let record = &tool.record;
        let arguments = if arguments.is_null() { json!({}) } else { arguments.clone() };
        let connection = connection(record);
        let called = self.hub.call(&connection, &record.settings, &tool.action, &arguments, &key);
        match self.runtime.block_on(called) {
            Ok(Value::String(text)) => text_result(text, false),
            Ok(value) => text_result(serde_json::to_string_pretty(&value).unwrap_or_default(), false),
            Err(error) => text_result(error.message, true),
        }
    }
}

impl McpEndpoint for ConnectorTools {
    fn handle(&self, message: &Value) -> Option<Value> {
        respond(
            message,
            &server(),
            || self.tools.iter().map(|tool| tool.definition.clone()).collect(),
            |name, arguments| match self.tools.iter().find(|tool| tool.definition["name"] == name) {
                Some(tool) => Ok(self.call(tool, arguments)),
                None => Err((-32602, format!("unknown tool: {name}"))),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use connectors::Idempotency;

    use super::*;

    fn record() -> ConnectionRecord {
        ConnectionRecord {
            id: "c1".into(),
            provider: "jira".into(),
            label: "Jira".into(),
            account: None,
            capabilities: vec!["tracker".into()],
            settings: json!({}),
        }
    }

    #[test]
    fn only_offered_actions_are_taken_and_writes_only_when_allowed() {
        let issue = ActionSpec::read("tracker.issue", json!({}), json!({}));
        let create = ActionSpec::write("tracker.create_issue", json!({}), json!({}), Idempotency::Lookup);
        let diff = ActionSpec::read("code.files_diff", json!({}), json!({}));
        let none: [String; 0] = [];
        let reading = Wanted { provider: "*", actions: &none, writes: false };
        assert!(reading.takes(&record(), &issue));
        assert!(!reading.takes(&record(), &create));
        assert!(!reading.takes(&record(), &diff));
        let tracker = ["tracker".to_owned()];
        let writing = Wanted { provider: "jira", actions: &tracker, writes: true };
        assert!(writing.takes(&record(), &create));
        let only = ["tracker.issue".to_owned()];
        assert!(!Wanted { provider: "jira", actions: &only, writes: true }.takes(&record(), &create));
    }

    #[test]
    fn a_tool_is_named_by_its_connection_and_action_within_the_limit() {
        assert_eq!(tool_name("Jira Prod", "tracker.issue"), "jira-prod_tracker_issue");
        assert_eq!(tool_name(&"x".repeat(80), "a.b").len(), NAME_LIMIT);
    }
}
