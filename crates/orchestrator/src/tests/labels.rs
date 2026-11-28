use connectors::descriptor::object;
use connectors::{ActionSpec, Idempotency};
use domain::label::{Trust, reads};
use domain::operation::{Kind, Profile, Reversible, Risk};
use domain::permission::{Allowed, allowed};
use serde_json::json;

use super::*;

const FORWARDING: &str = r#"
import { workflow } from "sdk";
export default workflow({
  name: "forwarding",
  async run(ctx) {
    const issue = await ctx.call("tracker", "issue", { key: "K-1" });
    try {
      const sent = await ctx.call("tracker", "comment", { issue: "K-2", text: issue.title });
      return { sent: sent.n };
    } catch (error) {
      return { kind: error.kind, message: error.message };
    }
  },
});
"#;

const ENFORCED: &str = r#"export const labels = { mode: "enforce", levels: ["public", "internal", "confidential"] };"#;

fn shaped(world: &World, untrusted: bool, output: Option<&str>, risk: Risk) {
    let reading = Profile::new(vec![Kind::Read], Risk::Low, Reversible::Yes, untrusted, true);
    let issue = ActionSpec {
        output_level: output.map(str::to_owned),
        ..ActionSpec::read("tracker.issue", object(&[], &[]), json!({}), reading)
    };
    let sending = Profile::new(vec![Kind::SendExternal], risk, Reversible::No, false, false);
    let comment = ActionSpec {
        max_input_level: Some("internal".into()),
        ..ActionSpec::write("tracker.comment", object(&[], &[]), json!({}), Idempotency::Lookup, sending)
    };
    world.provider.shape(issue);
    world.provider.shape(comment);
    world.provider.always("tracker.issue", json!({ "title": "salary table" }));
}

fn shipped_list() {
    allowed::install(Allowed::of(allowed::defaults()), Arc::new(|_: &Allowed| {}));
}

async fn forwarded(world: &World, input: Value) -> (Handle, ItemId) {
    world.workflow("forwarding", FORWARDING);
    let kernel = world.start().await;
    let id = kernel.create_item("forwarding", input).expect("item");
    (kernel, id)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_confidential_read_sent_past_an_internal_ceiling_asks_with_the_reason() {
    shipped_list();
    let world = World::new();
    world.write("safety/labels.ts", ENFORCED);
    shaped(&world, false, Some("confidential"), Risk::Medium);
    let (kernel, id) = forwarded(&world, json!({})).await;
    let asked = first_ask(&kernel).await;
    let AskKind::Permission { subject, rememberable, .. } = &asked.ask.kind else {
        panic!("a crossing is a permission: {asked:?}");
    };
    assert!(!rememberable);
    assert!(subject.contains("confidential data read from Fake tracker.issue past its internal ceiling"), "{subject}");
    assert_eq!(world.provider.count("tracker.comment"), 0, "nothing leaves before the answer");
    let item = kernel.item(&id).expect("item").expect("there");
    let read = reads(&item);
    assert_eq!(read.len(), 1);
    assert_eq!((read[0].label.as_deref(), read[0].trust), (Some("confidential"), Trust::System));
    kernel.answer_ask(&asked.ask.id, &Decision::Allow).expect("answer");
    assert_eq!(output(&ended(&kernel, &id).await), json!({ "sent": 1 }));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_risky_action_after_external_content_asks_and_a_routine_refuses_it() {
    shipped_list();
    let world = World::new();
    world.write("safety/labels.ts", ENFORCED);
    shaped(&world, true, Some("public"), Risk::High);
    let input = json!({ "schedule": { "subject": "schedule:forwarding:-:x" } });
    let (kernel, id) = forwarded(&world, input).await;
    let answer = output(&ended(&kernel, &id).await);
    let message = answer["message"].as_str().unwrap_or_default();
    assert!(message.starts_with("unavailable: "), "{answer}");
    assert!(message.contains("acts after reading external content from Fake tracker.issue"), "{answer}");
    assert!(kernel.asks().is_empty());
    assert_eq!(world.provider.count("tracker.comment"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn in_shadow_the_same_crossing_runs_as_before_and_the_read_is_still_kept() {
    shipped_list();
    let world = World::new();
    shaped(&world, true, Some("confidential"), Risk::High);
    let (kernel, id) = forwarded(&world, json!({})).await;
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!({ "sent": 1 }));
    assert!(kernel.asks().is_empty(), "the shipped levels only log");
    let read = reads(&item);
    assert_eq!((read[0].label.as_deref(), read[0].trust), (Some("confidential"), Trust::External));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unlabelled_read_counts_as_the_most_restricted_only_when_enforced() {
    shipped_list();
    let world = World::new();
    world.write("safety/labels.ts", ENFORCED);
    shaped(&world, false, None, Risk::Medium);
    let (kernel, _) = forwarded(&world, json!({})).await;
    let asked = first_ask(&kernel).await;
    let AskKind::Permission { subject, .. } = &asked.ask.kind else { panic!("a permission: {asked:?}") };
    assert!(subject.contains("would carry confidential data"), "{subject}");
}
