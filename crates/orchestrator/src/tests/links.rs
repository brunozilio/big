use serde_json::json;
use store::Application;

use super::*;

const STORY: &str = r#"import { workflow, step } from "sdk";
const merge = step({ name: "merge", async run(ctx) { await ctx.waitHuman([]); return ctx.pass({ sha: "abc" }); } });
export default workflow({ name: "story", steps: [merge({})] });
"#;

const BUNDLE: &str = r#"import { workflow, on } from "sdk";
export default workflow({
  name: "bundle",
  triggers: [on("item.step.finished", { data: { step: "merge", state: "passed" } }, { route: "accepting", title: "Bundle" })],
  async run(ctx) {
    let open = true;
    for (;;) {
      if (open) for (const id of await ctx.unclaimed("merge")) await ctx.link(id);
      const answer = await ctx.race([
        ctx.event("item.step.finished", { data: { step: "merge", state: "passed" } }),
        (branch) => branch.waitHuman([]),
      ]);
      if (answer.kind === "event") continue;
      const said = answer.value;
      if (said.action === "release") await ctx.unlink(said.values.item);
      if (said.action === "seal") { open = false; await ctx.accept(false); }
      if (said.action === "done") return { links: (await ctx.items([])).length };
    }
  },
});
"#;

fn world() -> World {
    let world = World::new();
    world.store().save_application(&Application { id: "app".into(), name: "Checkout".into(), ..Application::default() }).expect("app");
    world.workflow("story", STORY);
    world.workflow("bundle", BUNDLE);
    world
}

async fn merged(kernel: &Handle) -> ItemId {
    let story = kernel.create_item("story", json!({ "title": "Story", "application": { "id": "app" } })).expect("story");
    until(kernel, &story, WorkItem::waiting_human).await;
    kernel.human_input(&story, "merge", "ok", json!({})).expect("merge");
    until(kernel, &story, |item| item.is_finished()).await;
    story
}

async fn bundles(kernel: &Handle) -> Vec<WorkItem> {
    for _ in 0..200 {
        let found: Vec<WorkItem> =
            kernel.items().expect("items").into_iter().filter(|item| item.workflow == "bundle").map(|summary| {
                kernel.item(&summary.id).expect("store").expect("item")
            }).collect();
        if !found.is_empty() {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Vec::new()
}

#[tokio::test(flavor = "multi_thread")]
async fn merged_stories_gather_in_the_open_bundle_until_it_is_sealed_and_then_a_new_one_opens() {
    let world = world();
    let kernel = world.start().await;
    let first = merged(&kernel).await;
    let opened = bundles(&kernel).await;
    assert_eq!(opened.len(), 1);
    let bundle = opened[0].id.clone();
    let kept = until(&kernel, &bundle, |item| item.links == vec![first.clone()]).await;
    assert!(kept.accepting);
    assert_eq!(kept.input["title"], "Bundle");
    let second = merged(&kernel).await;
    until(&kernel, &bundle, |item| item.links.contains(&second)).await;
    assert_eq!(bundles(&kernel).await.len(), 1);
    until(&kernel, &bundle, WorkItem::waiting_human).await;
    kernel.human_input(&bundle, "", "release", json!({ "item": second.to_string() })).expect("release");
    let released = until(&kernel, &bundle, |item| item.released == vec![second.clone()]).await;
    assert_eq!(released.links, vec![first.clone()]);
    until(&kernel, &bundle, WorkItem::waiting_human).await;
    kernel.human_input(&bundle, "", "seal", json!({})).expect("seal");
    until(&kernel, &bundle, |item| !item.accepting).await;
    let third = merged(&kernel).await;
    let mut all = Vec::new();
    for _ in 0..200 {
        all = bundles(&kernel).await;
        if all.len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(all.len(), 2);
    let fresh = all.into_iter().find(|item| item.id != bundle).expect("a new bundle");
    let fresh = until(&kernel, &fresh.id, |item| item.links == vec![third.clone()]).await;
    assert!(fresh.accepting);
    let old = kernel.item(&bundle).expect("store").expect("item");
    assert!(!old.links.contains(&third));
}
