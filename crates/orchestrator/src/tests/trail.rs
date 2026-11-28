use std::collections::BTreeMap;
use std::sync::OnceLock;

use domain::audit::OperationTag;
use domain::permission::{Behavior, Grant, Rule};
use serde_json::json;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

use super::policy::{listing, tool};
use super::*;

type Fields = BTreeMap<String, String>;

#[derive(Default)]
struct Seen(Fields);

impl Visit for Seen {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }
}

struct Recorder;

fn kept() -> &'static Mutex<Vec<(String, Fields)>> {
    static KEPT: OnceLock<Mutex<Vec<(String, Fields)>>> = OnceLock::new();
    KEPT.get_or_init(Mutex::default)
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Recorder {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut seen = Seen::default();
        attrs.record(&mut seen);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(seen.0);
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if event.metadata().target() != "audit" {
            return;
        }
        let mut seen = Seen::default();
        event.record(&mut seen);
        for span in ctx.event_scope(event).into_iter().flatten() {
            if let Some(around) = span.extensions().get::<Fields>() {
                for (key, value) in around {
                    seen.0.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
        }
        let action = seen.0.remove("message").unwrap_or_default();
        kept().lock().expect("lock").push((action, seen.0));
    }
}

fn recording() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry().with(Recorder));
    });
}

async fn recorded(action: &str, operation: &str) -> Fields {
    let found = eventually(|| {
        let kept = kept().lock().expect("lock");
        kept.iter()
            .find(|(seen, fields)| seen == action && fields.get("operation.id").is_some_and(|id| id == operation))
            .map(|(_, fields)| fields.clone())
    })
    .await;
    found.unwrap_or_else(|| panic!("no {action} for {operation}: {:?}", kept().lock().expect("lock")))
}

const DECLARED: &str = r#"
import { workflow } from "sdk";
export default workflow({
  name: "declared",
  async run(ctx) {
    try {
      const operation = { id: "release.window.read", version: 2, friendlyName: "Read the release window" };
      await ctx.call("changes", "get", { change: "C-1" }, { operation });
      return "read";
    } catch (error) {
      return { kind: error.kind };
    }
  },
});
"#;

const UPDATING: &str = r#"
import { workflow } from "sdk";
export default workflow({
  name: "updating",
  async run(ctx) {
    await ctx.call("changes", "update", { change: "C-1", title: "t" });
    return "updated";
  },
});
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_blocked_call_records_its_decision_code_layer_and_the_authorization_stage() {
    recording();
    listing(vec![Rule::new(Grant::Tool(tool("changes.get")), Behavior::Deny)]);
    let world = World::new();
    world.workflow("declared", DECLARED);
    let kernel = world.start().await;
    let id = kernel.create_item("declared", json!({})).expect("item");
    assert_eq!(output(&ended(&kernel, &id).await), json!({ "kind": "permanent" }));
    let blocked = recorded("policy.blocked", "release.window.read").await;
    assert_eq!(blocked.get("decision.code").map(String::as_str), Some("list.denied"));
    assert_eq!(blocked.get("layer").map(String::as_str), Some("list"));
    assert_eq!(blocked.get("failure.stage").map(String::as_str), Some("authorization"));
    assert_eq!(blocked.get("operation.version").map(String::as_str), Some("2"));
    assert_eq!(blocked.get("operation.name").map(String::as_str), Some("Read the release window"));
    assert!(blocked.get("policy.version").is_some_and(|version| version.len() == 16), "{blocked:?}");
    assert_eq!(world.provider.count("changes.get"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approved_call_records_the_approval_and_how_long_it_waited() {
    recording();
    listing(vec![Rule::new(Grant::Tool(tool("changes.update")), Behavior::Ask)]);
    let world = World::new();
    let tag = OperationTag {
        id: "release.window.change".into(),
        version: Some("4".into()),
        friendly_name: Some("Change the release window".into()),
    };
    let spec = ActionSpec::read("changes.update", object(&[], &[]), json!({}), super::looked());
    world.provider.shape(ActionSpec { operation: Some(tag), ..spec });
    world.workflow("updating", UPDATING);
    let kernel = world.start().await;
    let id = kernel.create_item("updating", json!({})).expect("item");
    let asked = first_ask(&kernel).await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    kernel.answer_ask(&asked.ask.id, &Decision::Allow).expect("answer");
    assert_eq!(output(&ended(&kernel, &id).await), json!("updated"));
    let call = recorded("operation.call", "release.window.change").await;
    assert_eq!(call.get("authorized_by"), Some(&format!("approval:{}", asked.ask.id.0)));
    assert_eq!(call.get("decision.code").map(String::as_str), Some("approval.granted"));
    assert_eq!(call.get("operation.version").map(String::as_str), Some("4"));
    assert_eq!(call.get("outcome").map(String::as_str), Some("success"));
    assert_eq!(call.get("creator.type").map(String::as_str), Some("automation"));
    let waited: u64 = call.get("approval.wait_ms").and_then(|wait| wait.parse().ok()).expect("a wait");
    assert!(waited >= 30, "{call:?}");
}
