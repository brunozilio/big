use domain::permission::{Allowed, Behavior, Grant, Rule, allowed};
use serde_json::json;

use super::*;

const LINKING: &str = r#"
import { workflow } from "sdk";
export default workflow({
  name: "linking",
  async run(ctx) {
    try {
      const done = await ctx.call("tracker", "link_issues", { issue: "K-1", to: "K-2" });
      return { linked: done.n };
    } catch (error) {
      return { kind: error.kind, message: error.message };
    }
  },
});
"#;

const MERGING: &str = r#"
import { workflow } from "sdk";
export default workflow({
  name: "merging",
  async run(ctx) {
    const merged = await ctx.call("code", "merge", { pr: 7, method: "squash" });
    return { merged: merged.merged };
  },
});
"#;

fn tool(action: &str) -> String {
    domain::mcp::served_name(&domain::mcp::connectors_server(), &domain::mcp::connector_tool("Fake", action))
}

fn listing(rules: Vec<Rule>) {
    let mut list = Allowed::of(allowed::defaults());
    for rule in rules {
        list.add(rule);
    }
    allowed::install(list, Arc::new(|_: &Allowed| {}));
}

async fn linked(world: &World) -> (Handle, ItemId) {
    world.workflow("linking", LINKING);
    let kernel = world.start().await;
    let id = kernel.create_item("linking", json!({})).expect("item");
    (kernel, id)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deny_rule_in_the_list_stops_a_workflow_call_before_it_reaches_the_connector() {
    listing(vec![Rule::new(Grant::Tool(tool("tracker.link_issues")), Behavior::Deny)]);
    let world = World::new();
    let (kernel, id) = linked(&world).await;
    let answer = output(&ended(&kernel, &id).await);
    assert_eq!(answer["kind"], "permanent", "{answer}");
    let message = answer["message"].as_str().unwrap_or_default();
    assert!(message.contains("blocked_by_policy") && message.contains("denied by the permission list"), "{answer}");
    assert_eq!(world.provider.count("tracker.link_issues"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ask_rule_puts_a_permission_on_the_item_and_the_call_runs_once_a_person_allows_it() {
    listing(vec![Rule::new(Grant::Tool(tool("tracker.link_issues")), Behavior::Ask)]);
    let world = World::new();
    let (kernel, id) = linked(&world).await;
    let asked = first_ask(&kernel).await;
    assert_eq!(asked.item, id);
    let AskKind::Permission { tool: named, subject, rememberable, .. } = &asked.ask.kind else {
        panic!("a policy ask is a permission: {asked:?}");
    };
    assert_eq!(
        (named.as_str(), subject.as_str(), *rememberable),
        (tool("tracker.link_issues").as_str(), "tracker.link_issues", false)
    );
    assert_eq!(world.provider.count("tracker.link_issues"), 0, "nothing runs before the answer");
    kernel.answer_ask(&asked.ask.id, &Decision::Allow).expect("answer");
    assert_eq!(output(&ended(&kernel, &id).await), json!({ "linked": 1 }));
    assert!(kernel.asks().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_who_refuses_the_ask_blocks_the_call() {
    listing(vec![Rule::new(Grant::Tool(tool("tracker.link_issues")), Behavior::Ask)]);
    let world = World::new();
    let (kernel, id) = linked(&world).await;
    let asked = first_ask(&kernel).await;
    kernel.answer_ask(&asked.ask.id, &Decision::Deny).expect("answer");
    let answer = output(&ended(&kernel, &id).await);
    assert!(answer["message"].as_str().is_some_and(|told| told.contains("blocked_by_policy")), "{answer}");
    assert_eq!(world.provider.count("tracker.link_issues"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_autonomous_merge_runs_under_the_shipped_list_and_a_rule_that_allows_it_still_allows_it() {
    for rules in [Vec::new(), vec![Rule::new(Grant::Tool(tool("code.merge")), Behavior::Allow)]] {
        listing(rules);
        let world = World::new();
        world.provider.always("code.merge", json!({ "merged": true, "sha": "abc" }));
        world.workflow("merging", MERGING);
        let kernel = world.start().await;
        let id = kernel.create_item("merging", json!({})).expect("item");
        assert_eq!(output(&ended(&kernel, &id).await), json!({ "merged": true }));
        assert_eq!(world.provider.count("code.merge"), 1);
        assert!(kernel.asks().is_empty(), "the merge never asks");
    }
}
