use std::sync::Arc;

use app::policy_gate::{self, Asked};
use connectors::{ActionSpec, Connectors};
use domain::connection::{Capability, ProviderKey};
use domain::mcp::{LocalMcp, McpEndpoint, connector_tool, respond, served_name, slug, text_result};
use domain::operation::Kind;
use domain::permission::{Call, Effect, Surface, Verdict};
use domain::redact::Known;
use serde_json::{Value, json};
use store::ConnectionRecord;

use super::connector_flow::Guarded;
use crate::calls::connection;
use crate::state::Ctx;

const ANY: &str = "*";
const STAGE: &str = "tools/call";

pub struct Wanted<'a> {
    pub provider: &'a str,
    pub actions: &'a [String],
    pub writes: bool,
    pub connection: Option<&'a str>,
}

impl Wanted<'_> {
    fn reaches(&self, record: &ConnectionRecord) -> bool {
        let provider = self.provider == ANY || record.provider == self.provider;
        let chosen = self.connection.is_none_or(|chosen| {
            record.id == chosen
                || record.label.trim().eq_ignore_ascii_case(chosen.trim())
                || slug(&record.label) == slug(chosen)
        });
        provider && chosen
    }

    fn takes(&self, record: &ConnectionRecord, action: &ActionSpec) -> bool {
        let named = |given: &String| action.name == *given || action.name.starts_with(&format!("{given}."));
        let offered =
            Capability::of_action(&action.name).is_some_and(|capability| connection(record).offers(&capability));
        offered
            && !action.hidden()
            && (self.writes || !action.writes)
            && (self.actions.is_empty() || self.actions.iter().any(named))
    }
}

struct Tool {
    definition: Value,
    record: ConnectionRecord,
    action: String,
    declared: domain::operation::Declaration,
    spec: ActionSpec,
}

pub struct ConnectorTools {
    hub: Arc<Connectors>,
    runtime: tokio::runtime::Handle,
    tools: Vec<Tool>,
    session: Option<String>,
    known: Known,
}

fn definition(name: &str, provider: &str, record: &ConnectionRecord, action: &ActionSpec) -> Value {
    let what = action.description.trim();
    let what = if what.is_empty() { format!("Run `{}`.", action.name) } else { what.to_owned() };
    let changes = if action.writes { " It changes data there." } else { "" };
    let schema =
        if action.input_schema.is_object() { action.input_schema.clone() } else { json!({ "type": "object" }) };
    json!({
        "name": name,
        "description": format!("{what} Through the {provider} connection \"{}\".{changes}", record.label),
        "inputSchema": schema,
        "annotations": {
            "readOnlyHint": !action.writes,
            "destructiveHint": action.profile.effects.contains(&Kind::Destructive),
            "openWorldHint": action.profile.effects.contains(&Kind::SendExternal),
        },
        "_meta": { domain::operation::META: action.declaration() },
    })
}

fn tools_of(hub: &Connectors, records: Vec<ConnectionRecord>, wanted: &Wanted<'_>) -> Vec<Tool> {
    let mut tools = Vec::new();
    for record in records {
        if !wanted.reaches(&record) {
            continue;
        }
        let Ok(descriptor) = hub.descriptor(&ProviderKey::from(record.provider.as_str())) else { continue };
        for action in descriptor.actions.iter().filter(|action| wanted.takes(&record, action)) {
            let name = connector_tool(&record.label, &action.name);
            let definition = definition(&name, &descriptor.name, &record, action);
            let declared = action.declaration();
            let (spec, record) = (action.clone(), record.clone());
            tools.push(Tool { definition, record, action: action.name.clone(), declared, spec });
        }
    }
    tools
}

pub fn local(
    ctx: &Ctx,
    wanted: &Wanted<'_>,
    runtime: tokio::runtime::Handle,
    session: &str,
    known: Known,
) -> Option<LocalMcp> {
    let records = ctx.backend.store.connections_of(domain::principal::LOCAL_USER).unwrap_or_default();
    let hub = Arc::clone(&ctx.backend.connectors);
    let tools = tools_of(&hub, records, wanted);
    let session = Some(session).filter(|session| *session != super::EXTERNAL).map(str::to_owned);
    (!tools.is_empty())
        .then(|| LocalMcp::new(server(), Arc::new(ConnectorTools { hub, runtime, tools, session, known })))
}

fn server() -> String {
    domain::mcp::connectors_server()
}

const ASKED: Asked<'static> =
    Asked { surface: Surface::Mcp, stage: STAGE, owner: domain::principal::LOCAL_USER, operation: None };

fn served(tool: &Tool) -> String {
    served_name(&server(), tool.definition["name"].as_str().unwrap_or_default())
}

fn refused(tool: &Tool, arguments: &Value) -> Option<String> {
    let server = server();
    let name = served(tool);
    let effects: Vec<Effect> = tool.declared.operation(&server, arguments).map(Effect::Operation).into_iter().collect();
    let asked = Asked { operation: Some(&tool.action), ..ASKED };
    let subject = policy_gate::subject(&tool.action, arguments);
    match policy_gate::decide(asked, &Call { tool: &name, subject: &subject, effects: &effects }) {
        Verdict::Allow => None,
        Verdict::Deny(reason) => Some(policy_gate::refusal(&reason)),
        Verdict::Ask { .. } => Some(policy_gate::unasked(asked, &name)),
    }
}

impl ConnectorTools {
    fn call(&self, tool: &Tool, arguments: &Value) -> Value {
        let arguments = if arguments.is_null() { json!({}) } else { arguments.clone() };
        if let Some(refused) = refused(tool, &arguments) {
            return text_result(refused, true);
        }
        let Ok(key) = crate::attachments::random_id() else {
            return text_result("No idempotency key could be made.", true);
        };
        let key = format!("{}:{key}", brand::connectors::MARKER);
        let record = &tool.record;
        let connection = connection(record);
        let charged = domain::origin::Attribution::default();
        let called = async {
            let value = self
                .hub
                .call_for(Some(&charged), &connection, &record.settings, &tool.action, &arguments, &key)
                .await?;
            Ok(self.hub.labelled(&connection, &tool.action, value, &self.known))
        };
        let (name, levels) = (served(tool), app::labels::levels());
        let guarded = Guarded {
            asked: Asked { operation: Some(&tool.action), ..ASKED },
            session: self.session.as_deref(),
            tool: &name,
            spec: &tool.spec,
            levels: &levels,
        };
        match guarded.run(|| self.runtime.block_on(called)) {
            Ok(text) => text_result(text, false),
            Err(told) => text_result(told, true),
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
    use domain::operation::Visibility;

    use super::*;

    fn looked() -> domain::operation::Profile {
        use domain::operation::{Kind, Profile, Reversible, Risk};
        Profile::new(vec![Kind::Read], Risk::Low, Reversible::Yes, false, false)
    }

    fn changed() -> domain::operation::Profile {
        use domain::operation::{Kind, Profile, Reversible, Risk};
        Profile::new(vec![Kind::Write], Risk::Low, Reversible::Yes, false, false)
    }

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
        let issue = ActionSpec::read("tracker.issue", json!({}), json!({}), looked());
        let create = ActionSpec::write("tracker.create_issue", json!({}), json!({}), Idempotency::Lookup, changed());
        let diff = ActionSpec::read("code.files_diff", json!({}), json!({}), looked());
        let none: [String; 0] = [];
        let reading = Wanted { provider: "*", actions: &none, writes: false, connection: None };
        assert!(reading.takes(&record(), &issue));
        assert!(!reading.takes(&record(), &create));
        assert!(!reading.takes(&record(), &diff));
        let tracker = ["tracker".to_owned()];
        let writing = Wanted { provider: "jira", actions: &tracker, writes: true, connection: None };
        assert!(writing.takes(&record(), &create));
        let secret = ActionSpec::read("tracker.token", json!({}), json!({}), looked()).secret();
        assert!(!writing.takes(&record(), &secret));
        let host_only = ActionSpec {
            visibility: Visibility::Host,
            ..ActionSpec::read("tracker.sync", json!({}), json!({}), looked())
        };
        assert!(!writing.takes(&record(), &host_only), "an action meant for the host never reaches the model");
        let only = ["tracker.issue".to_owned()];
        assert!(!Wanted { provider: "jira", actions: &only, writes: true, connection: None }.takes(&record(), &create));
    }

    #[test]
    fn a_connection_is_chosen_by_id_name_or_slug() {
        let chosen = |connection: &str| {
            let wanted = Wanted { provider: "*", actions: &[], writes: true, connection: Some(connection) };
            wanted.reaches(&ConnectionRecord { label: "Jira Prod".into(), ..record() })
        };
        assert!(chosen("c1") && chosen(" jira prod ") && chosen("jira-prod"));
        assert!(!chosen("jira"));
        let other = Wanted { provider: "github", actions: &[], writes: true, connection: None };
        assert!(!other.reaches(&record()));
    }

    #[test]
    fn the_action_s_own_description_leads_and_writes_are_flagged() {
        let mut issue = ActionSpec::read("tracker.issue", json!({ "type": "object" }), json!({}), looked());
        let tool = definition("jira_tracker_issue", "Jira", &record(), &issue);
        assert_eq!(tool["description"], "Run `tracker.issue`. Through the Jira connection \"Jira\".");
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        issue.description = "Read one issue.".into();
        issue.writes = true;
        let tool = definition("jira_tracker_issue", "Jira", &record(), &issue);
        assert_eq!(tool["description"], "Read one issue. Through the Jira connection \"Jira\". It changes data there.");
        assert_eq!(tool["annotations"]["readOnlyHint"], false);
    }

    #[test]
    fn a_tool_is_named_by_its_connection_and_action_within_the_limit() {
        assert_eq!(connector_tool("Jira Prod", "tracker.issue"), "jira-prod_tracker_issue");
        assert_eq!(connector_tool(&"x".repeat(80), "a.b").len(), domain::mcp::CONNECTOR_TOOL_LIMIT);
    }
}

#[cfg(test)]
#[path = "connector_card_tests.rs"]
mod card_tests;
