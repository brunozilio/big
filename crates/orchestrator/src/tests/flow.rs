use serde_json::json;

use super::*;

const PLAIN: &str = r#"
import { workflow } from "sdk";

type Ran = { code: number; stdout: string };

export default workflow({
  name: "plain",
  async run(ctx, item) {
    const ran: Ran = await ctx.exec("printf oi");
    const issue = await ctx.call("tracker", "issue", { issue: "K-1" });
    const done = await ctx.agent("worker", `faça ${item.title}`);
    return { out: ran.stdout, call: issue.n, agent: done.text, session: done.session };
  },
});
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_typescript_workflow_runs_each_effect_once_and_replays_without_repeating_them() {
    let world = World::new();
    world.workflow("plain", PLAIN);
    let kernel = world.start().await;
    let id = kernel.create_item("plain", json!({ "title": "x" })).expect("item");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!({ "out": "oi", "call": 1, "agent": "feito 1", "session": "s1" }));
    let request = &world.engine.requests()[0];
    assert_eq!(request.prompt.text, "faça x");
    assert_eq!(request.policy.permission, domain::PermissionMode::Bypass);
    assert_eq!(request.session.append_system, None);
    let agent = request.session.agent.clone().expect("the step's agent runs as the main agent");
    assert_eq!(agent.name, "worker");
    assert_eq!(agent.definition.as_deref(), Some(WORKER), "its tools, skills and hooks travel with it");
    assert_ne!(request.host.credentials.client, 0, "each agent run has a client of its own");
    assert!(request.session.cwd.ends_with(git::folder_for(&git::branch_name(id.as_str()))));
    let keys = world.provider.keys("tracker.issue");
    assert_eq!(keys.len(), 1);
    assert!(keys[0].starts_with(&format!("{}:{id}/", brand::COMMAND)));
    crash(kernel).await;
    let again = world.start().await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(world.provider.count("tracker.issue"), 1, "a replay never calls again");
    assert_eq!(world.engine.runs(), 1);
    assert_eq!(again.item(&id).expect("store"), Some(item));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_effort_chosen_for_the_item_reaches_each_agent_request() {
    let world = World::new();
    world.workflow("plain", PLAIN);
    let kernel = world.start().await;
    let id = kernel.create_item("plain", json!({ "title": "x", "effort": "xhigh" })).expect("item");
    ended(&kernel, &id).await;
    assert_eq!(world.engine.requests()[0].session.effort, domain::Effort::XHigh);
    let other = kernel.create_item("plain", json!({ "title": "y" })).expect("item");
    ended(&kernel, &other).await;
    assert_eq!(world.engine.requests()[1].session.effort, domain::Effort::default());
}

const PARALLEL: &str = r#"
import { workflow } from "sdk";

export default workflow({
  name: "parallel",
  async run(ctx) {
    const both = await ctx.all([
      (left) => left.exec("printf a"),
      (right) => right.call("tracker", "issue", { issue: "K-2" }),
    ]);
    const raced = await ctx.race([ctx.event("tracker.status.changed", { to: "Cancelado" }), ctx.sleep(30)]);
    const first = await ctx.any([(one) => one.exec("exit 0"), (two) => two.sleep(10000)]);
    return { a: both[0].stdout, b: both[1].n, raced: raced.index, first: first.index };
  },
});
"#;

#[tokio::test(flavor = "multi_thread")]
async fn parallel_branches_keep_their_ids_and_the_losers_are_cancelled() {
    let world = World::new();
    world.workflow("parallel", PARALLEL);
    let kernel = world.start().await;
    let id = kernel.create_item("parallel", json!({})).expect("item");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!({ "a": "a", "b": 1, "raced": 1, "first": 0 }));
    let branches: Vec<String> = item.effects.values().filter_map(|entry| entry.request.key.branch.clone()).collect();
    assert!(branches.iter().any(|branch| branch == "main/0.0"), "{branches:?}");
    assert!(branches.iter().any(|branch| branch == "main/0.1"), "{branches:?}");
    let cancelled =
        item.effects.values().filter(|entry| entry.outcome == Some(domain::effect::EffectOutcome::Cancelled)).count();
    assert_eq!(cancelled, 2, "the event wait and the long sleep");
    assert!(item.timers.is_empty());
}

const PARENT: &str = r#"
import { workflow } from "sdk";
export default workflow({
  name: "parent",
  async run(ctx) {
    const got = await ctx.child("kid", { n: 2 });
    return { got };
  },
});
"#;

const KID: &str = r#"
import { workflow } from "sdk";
export default workflow({
  name: "kid",
  async run(ctx, item) {
    await ctx.exec("true");
    return { twice: item.input.n * 2 };
  },
});
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_child_is_an_item_of_its_own_and_the_parent_wakes_when_it_ends() {
    let world = World::new();
    world.workflow("parent", PARENT);
    world.workflow("kid", KID);
    let kernel = world.start().await;
    let id = kernel.create_item("parent", json!({})).expect("item");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!({ "got": { "twice": 4 } }));
    let link = item.children.first().expect("a child");
    assert!(link.finished);
    let child = kernel.item(&link.id).expect("store").expect("child");
    assert_eq!(child.parent.as_ref(), Some(&id));
    assert_eq!(child.workflow, "kid");
}

const TIMEOUT: &str = r#"
import { workflow } from "sdk";
export default workflow({
  name: "slow",
  async run(ctx) {
    try {
      await ctx.call("tracker", "issue", { issue: "K-3" }, { timeout: 80 });
      return "answered";
    } catch (error) {
      return error.kind;
    }
  },
});
"#;

#[tokio::test(flavor = "multi_thread")]
async fn an_effect_past_its_timeout_resolves_as_timeout() {
    let world = World::new();
    world.workflow("slow", TIMEOUT);
    world.provider.then("tracker.issue", Reply::Hang);
    let kernel = world.start().await;
    let id = kernel.create_item("slow", json!({})).expect("item");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!("timeout"));
    assert!(item.timers.is_empty());
}

const OVERRIDE: &str = r#"
import { workflow } from "sdk";
import { checks } from "sdk/steps";
export default workflow({ name: "mine", steps: [checks()] });
"#;

const CLIENT_CHECKS: &str = r#"
import { step } from "sdk";
import { tail } from "./context.js";
export default step({
  name: "checks",
  async run(ctx) {
    return ctx.pass({ who: tail("the client", 6) });
  },
});
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_client_step_replaces_the_built_in_one() {
    let world = World::new();
    world.workflow("mine", OVERRIDE);
    world.write("sdk/steps/checks.js", CLIENT_CHECKS);
    let kernel = world.start().await;
    let id = kernel.create_item("mine", json!({})).expect("item");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item)["data"], json!({ "who": "client" }));
    assert_eq!(world.provider.calls.lock().expect("lock").len(), 0);
    let view = kernel.step_view(&id, "checks", None).await.expect("view");
    assert_eq!(view.kind.as_deref(), Some("checks"));
    assert_eq!(view.attempts.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_built_in_delivery_workflow_loads_and_describes_its_steps() {
    let world = World::new();
    let kernel = world.start().await;
    assert!(kernel.workflows().contains(&"default".to_owned()));
    let shared = &kernel.shared;
    let script = shared.script("default").expect("the default workflow compiles");
    assert_eq!(script.target, json!({ "workflow": "entrega" }));
    let steps = script.described["workflows"][0]["steps"].as_array().map(Vec::len);
    assert_eq!(steps, Some(10));
}

const TEAM: &str = r#"
import { workflow } from "sdk";

export default workflow({
  name: "team",
  async run(ctx, item) {
    const done = await ctx.agent("worker", "chame a equipe");
    return { agent: done.text, session: done.session, output: done.output };
  },
});
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_teammate_message_after_the_turn_wakes_the_agent_in_the_same_session_before_the_step_ends() {
    let world = World::new();
    world.workflow("team", TEAM);
    let kernel = world.start().await;
    let id = kernel.create_item("team", json!({ "title": "x" })).expect("item");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!({ "agent": "feito 1\n\nfeito 2", "session": "s1", "output": { "run": 2 } }));
    let requests = world.engine.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].session.resume, Some(domain::SessionId("s1".into())));
    assert!(requests[1].prompt.text.contains("terminei a parte da equipe"), "{}", requests[1].prompt.text);
    let client = requests[0].host.credentials.client;
    assert_eq!(requests[1].host.credentials.client, client);
    assert_eq!(
        agent_engine::tools::clients::waiting(client, domain::PermissionMode::Ask, 0),
        agent_engine::tools::clients::Waiting::Idle,
        "the client is released when the step ends"
    );
}
