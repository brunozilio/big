use domain::factory::FindingKind;
use serde_json::json;

use super::*;
use crate::{FACTORY_ESCAPED, FACTORY_RECURRING};

const ESCAPING: &str = r#"
import { workflow, step } from "sdk";

const make = step({ name: "make", async run(ctx) { return ctx.pass({ branch: "item/x" }); } });
const checks = step({ name: "checks", async run(ctx) { return ctx.pass({ output: "ok" }); } });
const review = step({
  name: "review",
  async run(ctx) {
    if (ctx.from === null && ctx.output("review") === null) return ctx.goto("make", { body: "quebrou o login" });
    return ctx.pass({});
  },
});

export default workflow({ name: "escaping", steps: [make(), checks({ onFail: "make" }), review()] });
"#;

const FAILING: &str = r#"
import { workflow, step } from "sdk";

const only = step({ name: "only", async run(ctx) { return ctx.fail({ error: "sempre falha" }); } });

export default workflow({ name: "failing", steps: [only({ retries: 0 })] });
"#;

async fn factory_items(kernel: &Handle, workflow: &str) -> Vec<WorkItem> {
    eventually(|| {
        let found: Vec<WorkItem> = kernel
            .items()
            .expect("items")
            .iter()
            .filter_map(|summary| kernel.item(&summary.id).ok().flatten())
            .filter(|item| item.workflow == workflow)
            .collect();
        Some(found).filter(|found| !found.is_empty())
    })
    .await
    .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_gate_that_lets_a_defect_through_opens_the_meta_loop_once() {
    let world = World::new();
    world.workflow("escaping", ESCAPING);
    let kernel = world.start().await;
    let id = kernel.create_item("escaping", json!({ "title": "x" })).expect("item");
    let item = ended(&kernel, &id).await;
    assert!(item.input["generation"]["version"].as_str().is_some_and(|version| version.len() == 16));
    let opened = factory_items(&kernel, "factory-evals").await;
    assert_eq!(opened.len(), 1, "one escape opens one meta loop");
    let factory = &opened[0].input["factory"];
    assert_eq!(factory["kind"], FACTORY_ESCAPED);
    assert_eq!(factory["origin"], json!(id));
    assert_eq!(factory["data"]["gate"], "checks");
    assert_eq!(factory["data"]["caught_by"], "review");
    assert_eq!(factory["autonomy"], "near");
    assert_eq!(factory["dir"], brand::DIR);
    let store = world.store();
    assert_eq!(store.findings(Some(FindingKind::Escape)).expect("findings").len(), 1);
    let version = item.input["generation"]["version"].as_str().expect("version");
    assert!(store.generation(version).expect("generation").is_some());
    assert!(opened[0].input["generation"].is_object(), "the factory's own items are stamped too");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_that_keeps_failing_opens_the_outer_loop_at_the_threshold() {
    let world = World::new();
    world.workflow("failing", FAILING);
    let kernel = world.start().await;
    for round in 0..3 {
        let id = kernel.create_item("failing", json!({ "title": format!("t{round}") })).expect("item");
        until(&kernel, &id, |item| item.impediment.is_some()).await;
        if round < 2 {
            tokio::time::sleep(Duration::from_millis(150)).await;
            assert!(factory_none(&kernel, "factory-tune"), "below the threshold nothing opens");
        }
    }
    let opened = factory_items(&kernel, "factory-tune").await;
    assert_eq!(opened.len(), 1);
    let factory = &opened[0].input["factory"];
    assert_eq!(factory["kind"], FACTORY_RECURRING);
    assert_eq!(factory["data"]["step"], "only");
    assert_eq!(factory["data"]["count"], 3);
    assert_eq!(factory["data"]["findings"].as_array().map(Vec::len), Some(3));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_already_being_tuned_does_not_open_another_loop_at_the_next_multiple() {
    let world = World::new();
    world.workflow("failing", FAILING);
    let kernel = world.start().await;
    for round in 0..6 {
        let id = kernel.create_item("failing", json!({ "title": format!("t{round}") })).expect("item");
        until(&kernel, &id, |item| item.impediment.is_some()).await;
        if round == 2 {
            assert_eq!(factory_items(&kernel, "factory-tune").await.len(), 1);
        }
    }
    tokio::time::sleep(Duration::from_millis(150)).await;
    let opened = factory_items(&kernel, "factory-tune").await;
    assert_eq!(opened.len(), 1, "the open loop absorbs the next crossing");
    assert!(opened[0].ending.is_none());
}

fn factory_none(kernel: &Handle, workflow: &str) -> bool {
    kernel
        .items()
        .expect("items")
        .iter()
        .filter_map(|summary| kernel.item(&summary.id).ok().flatten())
        .all(|item| item.workflow != workflow)
}

fn put(dir: &Path, path: &str, text: &str) {
    let file = dir.join(path);
    std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
    std::fs::write(file, text).expect("write");
}

#[test]
fn a_repo_inherits_its_factory_chain_between_its_own_files_and_the_user_layer() {
    let dir = tempfile::tempdir().expect("dir");
    let repo = dir.path().join("repo");
    let user = dir.path().join("user");
    put(&repo, "factory", "acme\n");
    put(&repo, "agents/own.md", "repo");
    put(&user, "factories/acme/factory", "base");
    put(&user, "factories/acme/agents/shared.md", "acme");
    put(&user, "factories/acme/agents/own.md", "acme");
    put(&user, "factories/base/factory", "acme");
    put(&user, "factories/base/agents/root.md", "base");
    put(&user, "factories/base/agents/shared.md", "base");
    put(&user, "agents/root.md", "user");
    let builtin: crate::Builtin = Arc::new(|_| None);
    let resources = Resources::new(Some(repo), Some(user.clone()), builtin);
    assert_eq!(resources.chain(), ["acme".to_owned(), "base".to_owned()], "a cycle stops the chain");
    let text = |name: &str| resources.agent(name).map(|found| (found.origin, found.text));
    assert_eq!(text("own"), Some((crate::Origin::Repo, "repo".into())));
    assert_eq!(text("shared"), Some((crate::Origin::Factory, "acme".into())), "the nearest factory wins");
    assert_eq!(text("root"), Some((crate::Origin::Factory, "base".into())), "an ancestor still beats the user layer");
    assert_eq!(resources.factory_dir("acme"), Some(user.join("factories").join("acme")));
}

#[test]
fn a_generation_changes_when_any_factory_file_changes() {
    let dir = tempfile::tempdir().expect("dir");
    let repo = dir.path().join("repo");
    let user = dir.path().join("user");
    put(&repo, "factory", "acme");
    put(&user, "factories/acme/agents/a.md", "um");
    let builtin: crate::Builtin = Arc::new(|_| None);
    let resources = Resources::new(Some(repo), Some(user.clone()), builtin);
    let (first, files) = crate::factory::fingerprint(&resources);
    assert!(files.get("acme/agents/a.md").is_some());
    put(&user, "factories/acme/agents/a.md", "dois");
    let (second, _) = crate::factory::fingerprint(&resources);
    assert_ne!(first, second);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_built_in_loops_load_and_declare_their_triggers() {
    let world = World::new();
    let kernel = world.start().await;
    for (workflow, kind) in [("factory-tune", FACTORY_RECURRING), ("factory-evals", FACTORY_ESCAPED)] {
        let described = kernel.describe(workflow);
        let shape = &described["workflows"][0];
        assert_eq!(shape["name"], workflow);
        assert_eq!(shape["triggers"][0]["kind"], kind);
        assert!(shape["steps"].as_array().is_some_and(|steps| steps.len() >= 4), "{described}");
    }
}
