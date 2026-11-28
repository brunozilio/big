use domain::effect::Effect;
use domain::workflow::WorkItem;
use serde_json::json;

use super::*;

fn declared(world: &World) {
    world.provider.declare(json!([
        {
            "name": "fake.issue.created", "capability": "tracker", "source": "issue", "aliases": ["tracker.issue.created"],
            "schema": { "type": "object", "properties": { "title": { "type": "string" }, "type": { "type": "string" } } },
            "filters": ["type"]
        },
        {
            "name": "fake.status.changed", "capability": "tracker", "source": "issue", "aliases": ["tracker.status.changed"],
            "schema": { "type": "object", "properties": { "to": { "type": "string" } } }
        }
    ]));
}

fn triggered(world: &World, name: &str, kind: &str) {
    let source = format!(
        "import {{ workflow, on }} from \"sdk\";\nexport default workflow({{ name: \"{name}\", triggers: [on(\"{kind}\")], async run(ctx) {{ return await ctx.waitHuman([]); }} }});"
    );
    world.workflow(name, &source);
}

fn created(title: &str) -> connectors::ExternalEvent {
    let mut event = super::restart::event("t1", "fake.issue.created", json!({ "title": title, "type": "Bug" }));
    event.capability = Capability::from("tracker");
    event.subject = "BIG-9".into();
    event.aliases = vec!["tracker.issue.created".into()];
    event
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trigger_on_an_event_nobody_declares_fails_the_load_naming_the_closest() {
    let world = World::new();
    declared(&world);
    triggered(&world, "typo", "fake.issue.creatd");
    triggered(&world, "old", "tracker.issue.creatd");
    let kernel = world.start().await;
    for (workflow, closest) in [("typo", "fake.issue.created"), ("old", "tracker.issue.created")] {
        let error = kernel.loads(workflow).expect_err("refused").to_string();
        assert!(error.contains("no connector declares this event"), "{error}");
        assert!(error.contains(&format!("the closest is {closest}")), "{error}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trigger_on_the_old_name_and_on_the_new_one_both_fire_and_say_the_declared_source() {
    let world = World::new();
    declared(&world);
    triggered(&world, "renamed", "fake.issue.created");
    let kernel = world.start().await;
    assert_eq!(kernel.trigger(&created("Nova")).expect("trigger"), 1);
    let items = kernel.items().expect("items");
    let kept = kernel.item(&items[0].id).expect("store").expect("item");
    assert_eq!(kept.input["sources"][0]["kind"], "issue", "the source comes from the catalog, not the capability");
    assert_eq!(kept.input["issue"], "BIG-9");

    let world = World::new();
    declared(&world);
    triggered(&world, "legacy", "tracker.issue.created");
    let kernel = world.start().await;
    assert_eq!(kernel.trigger(&created("Antiga")).expect("trigger"), 1, "the capability name answers too");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trigger_on_a_namespace_no_loaded_connector_knows_still_loads() {
    let world = World::new();
    declared(&world);
    triggered(&world, "elsewhere", "billing.invoice.paid");
    let kernel = world.start().await;
    assert!(kernel.loads("elsewhere").is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wait_on_the_old_name_wakes_on_the_renamed_event() {
    let world = World::new();
    declared(&world);
    let body =
        "const e = await ctx.event(\"tracker.status.changed\", { to: \"Feito\" }); return ctx.pass({ kind: e.kind });";
    let source = format!(
        "import {{ workflow, step }} from \"sdk\";\nconst only = step({{ name: \"only\", async run(ctx) {{ {body} }} }});\nexport default workflow({{ name: \"waits\", steps: [only()] }});\n"
    );
    world.workflow("waits", &source);
    let kernel = world.start().await;
    let id = kernel.create_item("waits", json!({})).expect("item");
    let waiting =
        |item: &WorkItem| item.pending_effects().any(|request| matches!(request.effect, Effect::WaitEvent { .. }));
    until(&kernel, &id, waiting).await;
    let mut moved = super::restart::event("s1", "fake.status.changed", json!({ "to": "Feito" }));
    moved.aliases = vec!["tracker.status.changed".into()];
    assert_eq!(kernel.deliver_event(&moved).expect("deliver"), 1);
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item)["data"], json!({ "kind": "fake.status.changed" }));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wait_on_the_old_name_registered_after_the_event_takes_the_one_already_seen() {
    let world = World::new();
    declared(&world);
    let body = "const e = await ctx.event(\"tracker.status.changed\", { to: \"Feito\" }); return ctx.pass({ kind: e.kind, capability: e.capability });";
    let source = format!(
        "import {{ workflow, step }} from \"sdk\";\nconst only = step({{ name: \"only\", async run(ctx) {{ {body} }} }});\nexport default workflow({{ name: \"late\", steps: [only()] }});\n"
    );
    world.workflow("late", &source);
    let kernel = world.start().await;
    let id = kernel.create_item("late", json!({})).expect("item");
    let waiting =
        |item: &WorkItem| item.pending_effects().any(|request| matches!(request.effect, Effect::WaitEvent { .. }));
    until(&kernel, &id, waiting).await;
    crash(kernel).await;
    let seen = domain::workflow::Observed {
        id: "s1".into(),
        connection: "c1".into(),
        kind: "fake.status.changed".into(),
        subject: "BIG-9".into(),
        data: json!({ "to": "Feito" }),
        at: jiff::Timestamp::now().as_millisecond(),
        aliases: vec!["tracker.status.changed".into()],
        capability: "tracker".into(),
    };
    world.store().append(&id, &[domain::workflow::WorkflowEvent::ExternalObserved(seen)]).expect("observed");
    let kernel = world.start().await;
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item)["data"], json!({ "kind": "fake.status.changed", "capability": "tracker" }));
}
