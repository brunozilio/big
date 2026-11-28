use std::sync::atomic::Ordering;

use domain::effect::EffectOutcome;
use serde_json::json;

use super::*;

const BOTH: &str = r#"
import { workflow } from "sdk";
export default workflow({
  name: "both",
  async run(ctx) {
    const [made, read] = await Promise.all([ctx.agent("worker", "faça"), ctx.call("tracker", "issue", { key: "K-1" })]);
    return { made: made.text, read: read.n };
  },
});
"#;

const OTHER: &str = "---\nname: other\npermissionMode: auto\nmodel: auto\n---\nVocê revisa.";

fn alone(agent: &str) -> String {
    format!(
        "import {{ workflow }} from \"sdk\";\nexport default workflow({{ name: \"{agent}\", async run(ctx) {{ return (await ctx.agent(\"{agent}\", \"faça\")).text; }} }});"
    )
}

fn open(item: &WorkItem) -> usize {
    item.effects.values().filter(|entry| entry.outcome.is_none()).count()
}

async fn running(kernel: &Handle, id: &ItemId) -> WorkItem {
    until(kernel, id, |item| item.effects.values().any(|entry| entry.progress["session"].is_string())).await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_kill_switch_ends_the_run_and_marks_every_effect_in_flight_once() {
    let world = World::new();
    world.engine.state.hold.store(true, Ordering::SeqCst);
    world.provider.then("tracker.issue", Reply::Hang);
    world.workflow("both", BOTH);
    let kernel = world.start().await;
    let id = kernel.create_item("both", json!({})).expect("item");
    running(&kernel, &id).await;
    eventually(|| (world.provider.count("tracker.issue") == 1).then_some(())).await.expect("the call is in flight");
    let before = kernel.item(&id).expect("read").expect("item");
    assert_eq!(open(&before), 2, "the agent and the call are both in flight");

    kernel.kill(&id).expect("kill");
    let killed = kernel.item(&id).expect("read").expect("item");
    assert!(matches!(killed.ending, Some(domain::workflow::Ending::Cancelled { .. })), "{:?}", killed.ending);
    assert_eq!(open(&killed), 0, "no effect is left half done");
    assert!(killed.effects.values().all(|entry| entry.outcome == Some(EffectOutcome::Cancelled)));
    assert!(killed.timers.keys().all(|timer| !timer.starts_with("timeout:")), "{:?}", killed.timers);
    eventually(|| (world.engine.state.cancels.load(Ordering::SeqCst) >= 1).then_some(())).await.expect("cancelled");

    let late = killed.effects.values().next().map(|entry| entry.request.key.clone()).expect("key");
    let value = json!({ "text": "tarde" });
    assert!(
        !kernel.shared.resolve(&id, &late, EffectOutcome::Ok { value }).expect("resolve"),
        "a late result is not kept"
    );
    kernel.kill(&id).expect("again");
    kernel.cancel(&id).expect("cancel after");
    world.engine.state.hold.store(false, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let after = kernel.item(&id).expect("read").expect("item");
    assert_eq!(after.version, killed.version, "killing twice changes nothing");
    assert_eq!(after.effects, killed.effects);
    assert_eq!(world.engine.runs(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn killing_an_agent_stops_every_run_of_it_and_leaves_the_others() {
    let world = World::new();
    world.write("agents/other.md", OTHER);
    world.engine.state.hold.store(true, Ordering::SeqCst);
    world.workflow("worker", &alone("worker"));
    world.workflow("other", &alone("other"));
    let kernel = world.start().await;
    let first = kernel.create_item("worker", json!({})).expect("first");
    let second = kernel.create_item("worker", json!({})).expect("second");
    let third = kernel.create_item("other", json!({})).expect("third");
    for id in [&first, &second, &third] {
        running(&kernel, id).await;
    }
    let mut killed = kernel.kill_agent("worker").expect("kill");
    killed.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut wanted = vec![first.clone(), second.clone()];
    wanted.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    assert_eq!(killed, wanted);
    for id in [&first, &second] {
        let item = kernel.item(id).expect("read").expect("item");
        assert!(item.is_ended() && open(&item) == 0);
    }
    let spared = kernel.item(&third).expect("read").expect("item");
    assert!(!spared.is_ended());
    assert!(kernel.kill_agent("worker").expect("again").is_empty());
    world.engine.state.hold.store(false, Ordering::SeqCst);
    assert!(output(&ended(&kernel, &third).await).as_str().is_some_and(|text| text.starts_with("feito")));
}
