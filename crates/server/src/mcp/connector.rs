use std::sync::Arc;

use connectors::{ActionSpec, Connectors};
use domain::connection::{Capability, ProviderKey};
use domain::mcp::{LocalMcp, McpEndpoint, respond, slug, text_result};
use domain::operation::Kind;
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
    domain::mcp::connectors_server()
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
        assert_eq!(tool_name("Jira Prod", "tracker.issue"), "jira-prod_tracker_issue");
        assert_eq!(tool_name(&"x".repeat(80), "a.b").len(), NAME_LIMIT);
    }

    #[test]
    fn the_approval_card_of_a_sending_action_shows_its_effects_and_recipients() {
        use connectors::Provider;
        let folder = tempfile::tempdir().expect("folder");
        let dir = folder.path().join("connectors/fixture");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("index.ts"), FIXTURE).expect("fixture");
        let fixture =
            connectors::scripted::ScriptedProvider::load_in(Some(folder.path()), "connectors/fixture/index.ts")
                .expect("fixture");
        let share = fixture.descriptor().action("notes.share").cloned().expect("notes.share");
        let tool = definition("fixture_notes_share", "Fixture", &record(), &share);
        assert_eq!(tool["annotations"]["openWorldHint"], true);
        let declared: domain::operation::Declaration =
            serde_json::from_value(tool["_meta"][domain::operation::META].clone()).expect("declared");
        let input = json!({ "to": ["ana@example.com", "rui@example.com"], "body": "…" });
        let operation = declared.operation(&server(), &input).expect("sending acts");
        let disclosed = domain::operation::Disclosed::of([&operation]);
        let kind = domain::AskKind::Permission {
            tool: "fixture_notes_share".into(),
            subject: "…".into(),
            rememberable: false,
            disclosed,
        };
        let card = crate::session_view::ask_event(&domain::AgentAsk { id: domain::AskId("a".into()), kind }, 0);
        assert_eq!(card["effects"], json!(["send-external", "pii"]));
        assert_eq!(card["recipients"], json!(["ana@example.com", "rui@example.com"]));
        assert_eq!(card["risk"], "high");
    }

    const FIXTURE: &str = r#"import { object, write } from "sdk/connectors/contracts.ts";
export function provider() {
  const share = { effects: ["send-external", "pii"], risk: "high", reversible: false, untrusted: false, sensitive: true, recipients: ["to"] };
  return { schema_version: 2, name: "Fixture", capabilities: ["notes"], auth: { kind: "token", fields: [] },
    actions: [write("notes.share", object([["to", "array"], ["body", "string"]]), object([]), "native", share)] };
}
export function call() { return null; }
"#;

    fn verdicts(effects: &[domain::permission::Effect]) -> Vec<domain::permission::Verdict> {
        use domain::permission::{Allowed, Bounds, Call, Grant, PolicyContext, PolicyEngine, Remembered};
        let bounds = Bounds::of(Vec::new(), Vec::new());
        let remembered = Remembered::default();
        let listed = Allowed::of([Grant::Tool("t".into())]);
        let mut found = Vec::new();
        for mode in domain::PermissionMode::ALL {
            for allowed in [None, Some(&listed)] {
                for no_prompts in [false, true] {
                    let call = Call { tool: "t", subject: "", effects };
                    let cx = PolicyContext {
                        mode,
                        ceiling: None,
                        remembered: &remembered,
                        bounds: &bounds,
                        no_prompts,
                        allowed,
                    };
                    found.push(cx.settled(PolicyEngine::decide(&call, &cx), "t"));
                }
            }
        }
        found
    }

    #[test]
    fn shipped_actions_are_judged_as_before_except_the_ones_that_declare_a_strict_effect() {
        use domain::permission::Effect;
        let mut changed = Vec::new();
        for provider in connectors::scripted::discover() {
            for action in provider.descriptor().actions.iter().filter(|action| !action.hidden()) {
                let tool = definition("t", "P", &record(), action);
                let declared: domain::operation::Declaration =
                    serde_json::from_value(tool["_meta"][domain::operation::META].clone()).expect("declared");
                let now: Vec<Effect> =
                    declared.operation(&server(), &json!({})).map(Effect::Operation).into_iter().collect();
                let before: Vec<Effect> =
                    if action.writes { vec![Effect::Remote { server: server() }] } else { Vec::new() };
                if verdicts(&now) != verdicts(&before) {
                    changed.push(action.name.clone());
                }
            }
        }
        changed.sort();
        changed.dedup();
        assert_eq!(changed, ["email.create_event", "email.reply", "email.send"]);
    }
}
