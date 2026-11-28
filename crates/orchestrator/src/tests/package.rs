use serde_json::json;
use store::{Application, Binding, Bindings, Repository};

use super::*;

const STORY: &str = r#"import { workflow, step } from "sdk";
import { branches } from "../branches.ts";
const merge = step({ name: "merge", async run(ctx) { await ctx.waitHuman([]); return ctx.pass({ sha: "s1" }); } });
export default workflow({ name: "default", branches, steps: [merge({})] });
"#;

fn world() -> World {
    let world = World::new();
    let application = Application {
        id: "app".into(),
        name: "Checkout".into(),
        bindings: Bindings::from([
            ("tracker".to_owned(), Binding::new("c1", "APP")),
            ("changes".to_owned(), Binding::new("c1", "mudancas")),
        ]),
        repositories: vec![Repository {
            id: "web".into(),
            bindings: Bindings::from([("code".to_owned(), Binding::new("c1", "o/web"))]),
            directory: world.repo().to_string_lossy().into_owned(),
            ..Repository::default()
        }],
        ..Application::default()
    };
    world.store().save_application(&application).expect("app");
    world.workflow("default", STORY);
    let fake = world.provider;
    fake.always("code.open_pull_request", json!({ "ref": "o/web#5" }));
    fake.always("code.merge", json!({ "merged": true, "sha": "abc1234ff" }));
    fake.always("pipelines.runs", json!([{ "ref": "o/web/runs/9", "state": "passed", "head": "old9999" }]));
    fake.always("tracker.create_issue", json!({ "ref": "BIG-50", "status": "Aberto" }));
    fake.always("changes.create", json!({ "ref": "mock-1", "key": "GMUD-1", "status": "awaiting_approval" }));
    fake.always("changes.update", json!({ "ref": "mock-1", "key": "GMUD-1", "status": "awaiting_approval" }));
    fake.always("changes.get", json!({ "ref": "mock-1", "key": "GMUD-1", "status": "done" }));
    world
}

async fn merged(kernel: &Handle, title: &str) -> ItemId {
    merged_with(kernel, title, json!({ "web": { "head": "feat/x", "base": "develop" } })).await
}

async fn merged_with(kernel: &Handle, title: &str, git: Value) -> ItemId {
    let bound = json!({ "tracker": { "connection": "c1", "value": "APP" }, "changes": { "connection": "c1", "value": "mudancas" } });
    merged_in(kernel, title, git, bound).await
}

async fn merged_in(kernel: &Handle, title: &str, git: Value, bindings: Value) -> ItemId {
    let repositories = json!([{ "id": "web", "bindings": { "code": { "connection": "c1", "value": "o/web" } } }]);
    let application = json!({ "id": "app", "bindings": bindings, "repositories": repositories });
    let input = json!({ "title": title, "issue": title, "application": application, "git": git });
    let story = kernel.create_item("default", input).expect("story");
    until(kernel, &story, WorkItem::waiting_human).await;
    kernel.human_input(&story, "merge", "ok", json!({})).expect("merge");
    until(kernel, &story, |item| item.is_finished()).await;
    story
}

async fn packages(kernel: &Handle, count: usize) -> Vec<ItemId> {
    let found = eventually(|| {
        let found: Vec<ItemId> = kernel
            .items()
            .expect("items")
            .into_iter()
            .filter(|item| item.workflow == "package")
            .map(|item| item.id)
            .collect();
        Some(found).filter(|found| found.len() >= count)
    })
    .await;
    found.unwrap_or_else(|| panic!("there were never {count} packages"))
}

fn stage(item: &WorkItem, env: &str) -> Value {
    let Some(stages) = block(item, "stages") else { return Value::Null };
    stages["stages"].as_array().and_then(|all| all.iter().find(|one| one["id"] == env)).cloned().unwrap_or(Value::Null)
}

fn block(item: &WorkItem, kind: &str) -> Option<Value> {
    item.steps
        .iter()
        .filter(|run| run.id == "promotion" && run.state == domain::workflow::StepState::WaitingHuman)
        .filter_map(|run| {
            run.view.as_array().and_then(|blocks| blocks.iter().find(|block| block["type"] == kind)).cloned()
        })
        .next_back()
}

async fn answer(kernel: &Handle, id: &ItemId, action: &str, values: Value) -> WorkItem {
    let before = kernel.item(id).expect("store").expect("item").version;
    until(kernel, id, |item| item.waiting_human() && item.version >= before).await;
    kernel.human_input(id, "promotion", action, values).expect("input");
    until(kernel, id, |item| item.waiting_human() && !item.inputs.is_empty() && item.version > before + 2).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_package_gathers_stories_goes_to_internal_homologation_and_needs_a_change_for_each_outside_environment() {
    let world = world();
    let fake = world.provider;
    let kernel = world.start().await;
    let first = merged(&kernel, "APP-1").await;
    let package = packages(&kernel, 1).await.remove(0);
    let item =
        until(&kernel, &package, |item| item.is_ended() || (item.links == vec![first.clone()] && item.waiting_human()))
            .await;
    assert!(!item.is_ended(), "{:?} {:?}", item.ending, item.steps);
    assert_eq!(stage(&item, "hint")["state"], "passed");
    assert_eq!(stage(&item, "hext")["picker"], true);
    assert!(stage(&item, "prod")["window"].is_null() && stage(&item, "prod")["picker"] == false);
    assert_eq!(fake.count("code.merge"), 1);
    assert_eq!(fake.count("tracker.create_issue"), 1);
    let item = answer(&kernel, &package, "window", json!({ "env": "hext", "at": "2026-11-26T22:00" })).await;
    assert_eq!(stage(&item, "hext")["actions"][0]["id"], "create_change");
    answer(&kernel, &package, "create_change", json!({ "env": "hext" })).await;
    let item = until(&kernel, &package, |item| !item.accepting).await;
    assert_eq!(fake.count("changes.create"), 1);
    assert!(fake.count("pipelines.runs") >= 1);
    let created =
        fake.calls.lock().expect("lock").iter().find(|(name, _, _)| name == "changes.create").cloned().expect("create");
    assert_eq!(created.1["environment"], "hext");
    assert_eq!(created.1["tasks"][0]["start"], "22:00");
    assert_eq!(created.1["start"], "2026-11-26T22:00:00-03:00");
    assert!(created.1["end"].as_str().is_some_and(|end| end.ends_with("-03:00") && end > "2026-11-26T22:00"));
    assert!(item.links.contains(&first));
    let second = merged(&kernel, "APP-2").await;
    let all = packages(&kernel, 2).await;
    let other = all.into_iter().find(|id| id != &package).expect("a second package");
    until(&kernel, &other, |item| item.links == vec![second.clone()]).await;
    let updates = fake.count("tracker.update_issue");
    kernel
        .deliver_event(&connectors::ExternalEvent {
            id: "mock-1:done".into(),
            connection: ConnectionId::from("c1"),
            capability: Capability::from("changes"),
            kind: "changes.status.changed".into(),
            subject: "mock-1".into(),
            data: json!({ "ref": "mock-1", "status": "done" }),
            at: jiff::Timestamp::UNIX_EPOCH,
            scope: String::new(),
            aliases: Vec::new(),
        })
        .expect("deliver");
    let item = until(&kernel, &package, |item| item.waiting_human() && stage(item, "hext")["state"] == "passed").await;
    assert_eq!(stage(&item, "prod")["picker"], true);
    assert!(fake.count("tracker.update_issue") > updates);
    answer(&kernel, &package, "window", json!({ "env": "prod", "at": "2026-11-27T23:00" })).await;
    answer(&kernel, &package, "create_change", json!({ "env": "prod" })).await;
    let last = fake
        .calls
        .lock()
        .expect("lock")
        .iter()
        .rfind(|(name, _, _)| name == "changes.create")
        .cloned()
        .expect("create");
    assert_eq!((last.1["environment"].clone(), last.1["tasks"][0]["start"].clone()), (json!("prod"), json!("23:00")));
    let item = until(&kernel, &package, WorkItem::waiting_human).await;
    let tasks = block(&item, "tasks").expect("tasks");
    assert_eq!(tasks["locked"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_package_built_by_hand_starts_empty_and_takes_only_what_is_moved_into_it() {
    let world = world();
    let kernel = world.start().await;
    let first = merged(&kernel, "APP-1").await;
    let automatic = packages(&kernel, 1).await.remove(0);
    until(&kernel, &automatic, |item| item.links == vec![first.clone()] && item.waiting_human()).await;
    let manual = kernel.create_item("package", json!({ "title": "Pacote" })).expect("manual");
    until(&kernel, &manual, WorkItem::waiting_human).await;
    let second = merged(&kernel, "APP-2").await;
    until(&kernel, &automatic, |item| item.links.contains(&second)).await;
    assert!(kernel.item(&manual).expect("store").expect("item").links.is_empty());
    answer(&kernel, &automatic, "unlink", json!({ "item": second.to_string() })).await;
    let moved = answer(&kernel, &manual, "link", json!({ "items": [second.to_string()] })).await;
    assert_eq!(moved.links, vec![second.clone()]);
    let left = kernel.item(&automatic).expect("store").expect("item");
    assert_eq!((left.links.clone(), left.released.clone()), (vec![first], vec![second]));
    assert!(!moved.accepting);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_package_promotes_each_repository_along_the_flow_its_repository_follows() {
    let world = world();
    let fake = world.provider;
    let kernel = world.start().await;
    let promote = json!([{ "from": "dev", "into": "homolog", "method": "squash" }]);
    let story =
        merged_with(&kernel, "APP-3", json!({ "web": { "head": "feature/APP-3", "base": "dev", "promote": promote } }))
            .await;
    let package = packages(&kernel, 1).await.remove(0);
    let item = until(&kernel, &package, |item| item.is_ended() || stage(item, "hint")["state"] == "passed").await;
    assert!(item.links.contains(&story), "the delivery arrives through its own flow's integration branch");
    let calls = fake.calls.lock().expect("lock").clone();
    let opened = calls.iter().find(|(name, _, _)| name == "code.open_pull_request").expect("a promotion");
    assert_eq!((opened.1["head"].clone(), opened.1["base"].clone()), (json!("dev"), json!("homolog")));
    let merge = calls.iter().find(|(name, _, _)| name == "code.merge").expect("merged");
    assert_eq!(merge.1["method"], "squash");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_package_of_an_application_without_a_tracker_project_opens_no_card() {
    let world = world();
    let mut application = world.store().application("app").expect("store").expect("app");
    application.bindings.clear();
    world.store().save_application(&application).expect("unbound");
    let kernel = world.start().await;
    merged_in(&kernel, "APP-1", json!({ "web": { "head": "feat/x", "base": "develop" } }), json!({})).await;
    let package = packages(&kernel, 1).await.remove(0);
    let item =
        until(&kernel, &package, |item| item.is_ended() || item.waiting_human() || item.impediment.is_some()).await;
    assert!(item.impediment.is_none(), "{:?}", item.impediment);
    assert_eq!(world.provider.count("tracker.create_issue"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_change_tool_bound_a_person_approves_the_environment_here_and_nothing_reaches_a_tool() {
    let world = world();
    let fake = world.provider;
    let mut application = world.store().application("app").expect("store").expect("app");
    application.bindings.remove("changes");
    world.store().save_application(&application).expect("no changes");
    let kernel = world.start().await;
    let tracker = json!({ "tracker": { "connection": "c1", "value": "APP" } });
    merged_in(&kernel, "APP-1", json!({ "web": { "head": "feat/x", "base": "develop" } }), tracker).await;
    let package = packages(&kernel, 1).await.remove(0);
    until(&kernel, &package, |item| item.waiting_human() && stage(item, "hext")["picker"] == true).await;
    answer(&kernel, &package, "window", json!({ "env": "hext", "at": "2026-11-26T22:00" })).await;
    let item = answer(&kernel, &package, "create_change", json!({ "env": "hext" })).await;
    let actions: Vec<Value> = stage(&item, "hext")["actions"].as_array().cloned().unwrap_or_default();
    let ids: Vec<&str> = actions.iter().filter_map(|action| action["id"].as_str()).collect();
    assert_eq!(ids, ["approve_change", "reject_change"]);
    let before = kernel.item(&package).expect("store").expect("item").version;
    kernel.human_input_by(&package, "promotion", "approve_change", json!({ "env": "hext" }), "Ana").expect("approve");
    let item = until(&kernel, &package, |item| item.waiting_human() && item.version > before + 2).await;
    assert_eq!(stage(&item, "hext")["state"], "passed");
    assert_eq!(stage(&item, "hext")["change"]["label"], "executada · Ana");
    assert_eq!(stage(&item, "prod")["picker"], true);
    let reached: Vec<String> = fake
        .calls
        .lock()
        .expect("lock")
        .iter()
        .map(|(name, _, _)| name.clone())
        .filter(|name| name.starts_with("changes."))
        .collect();
    assert_eq!(reached, Vec::<String>::new());
}
