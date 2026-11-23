use std::time::Duration;

use domain::effect::Effect;
use domain::workflow::{StepState, WorkItem};
use serde_json::json;

use super::*;

fn single(kind: &str, body: &str) -> String {
    format!(
        "import {{ workflow, step }} from \"sdk\";\nconst only = step({{ name: \"only\", async run(ctx) {{ {body} }} }});\nexport default workflow({{ name: \"{kind}\", steps: [only()] }});\n"
    )
}

fn requested(item: &WorkItem, kind: fn(&Effect) -> bool) -> usize {
    item.effects.values().filter(|entry| kind(&entry.request.effect)).count()
}

async fn interrupted(world: &World, workflow: &str, ready: impl Fn(&WorkItem) -> bool) -> (Handle, ItemId) {
    let kernel = world.start().await;
    let id = kernel.create_item(workflow, json!({ "title": "t" })).expect("item");
    until(&kernel, &id, ready).await;
    crash(kernel).await;
    (world.start().await, id)
}

#[tokio::test(flavor = "multi_thread")]
async fn an_exec_killed_midway_runs_again_with_the_same_key() {
    let world = World::new();
    let log = world.dir.path().join("runs.log");
    let marker = world.dir.path().join("marker");
    let command = format!(
        "printf run >> {log}; if [ -f {marker} ]; then printf done; else touch {marker}; sleep 30; fi",
        log = log.display(),
        marker = marker.display()
    );
    let body = format!("const r = await ctx.exec({}); return ctx.pass({{ out: r.stdout }});", json!(command));
    world.workflow("exec", &single("exec", &body));
    let (kernel, id) = interrupted(&world, "exec", |_| marker.exists()).await;
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item)["data"], json!({ "out": "done" }));
    assert_eq!(std::fs::read_to_string(&log).expect("log"), "runrun");
    assert_eq!(requested(&item, |effect| matches!(effect, Effect::Exec { .. })), 1, "one key, run twice");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_killed_midway_is_repeated_with_the_same_idempotency_key() {
    let world = World::new();
    world.provider.then("tracker.issue", Reply::Hang);
    let body = "const r = await ctx.call(\"tracker\", \"issue\", { issue: \"K\" }); return ctx.pass(r);";
    world.workflow("call", &single("call", body));
    let (kernel, id) = interrupted(&world, "call", |_| world.provider.count("tracker.issue") == 1).await;
    let item = ended(&kernel, &id).await;
    let keys = world.provider.keys("tracker.issue");
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0], keys[1]);
    assert_eq!(output(&item)["data"]["n"], 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_killed_midway_resumes_its_session_and_is_told_to_continue() {
    let world = World::new();
    world.engine.state.hold.store(true, Ordering::SeqCst);
    let body =
        "const r = await ctx.agent(\"worker\", \"faça\"); return ctx.pass({ session: r.session, text: r.text });";
    world.workflow("agent", &single("agent", body));
    let started = |item: &WorkItem| item.effects.values().any(|entry| !entry.progress.is_null());
    let (kernel, id) = interrupted(&world, "agent", started).await;
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item)["data"], json!({ "session": "s1", "text": "feito 2" }));
    let requests = world.engine.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].session.resume, Some(domain::SessionId("s1".into())));
    assert_eq!(requests[1].prompt.text, crate::agent::RESUME);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_human_wait_survives_a_restart_and_resolves_with_the_answer() {
    let world = World::new();
    let body =
        "const a = await ctx.waitHuman([actions([\"approve\"])]); return ctx.pass({ chose: a.action, v: a.values });";
    let source = single("human", body).replace("import { workflow, step }", "import { workflow, step, actions }");
    world.workflow("human", &source);
    let waiting = |item: &WorkItem| item.waiting_human();
    let (kernel, id) = interrupted(&world, "human", waiting).await;
    until(&kernel, &id, waiting).await;
    let view = kernel.step_view(&id, "only", None).await.expect("view");
    assert_eq!(view.blocks[0]["type"], "actions");
    assert!(kernel.human_input(&id, "elsewhere", "approve", json!({ "ok": 1 })).is_ok(), "the only wait is found");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item)["data"], json!({ "chose": "approve", "v": { "ok": 1 } }));
    assert_eq!(item.inputs.len(), 1);
    assert_eq!(item.steps[0].state, StepState::Passed);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_event_wait_is_registered_again_after_a_restart() {
    let world = World::new();
    let body = "const e = await ctx.event(\"pipelines.run.finished\", { run: \"r1\" }); return ctx.pass({ id: e.id });";
    world.workflow("event", &single("event", body));
    let waiting =
        |item: &WorkItem| item.pending_effects().any(|request| matches!(request.effect, Effect::WaitEvent { .. }));
    let (kernel, id) = interrupted(&world, "event", waiting).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let other = event("e0", "pipelines.run.finished", json!({ "run": "r2" }));
    assert_eq!(kernel.deliver_event(&other).expect("deliver"), 0);
    let hit = event("e1", "pipelines.run.finished", json!({ "run": "r1" }));
    assert_eq!(kernel.deliver_event(&hit).expect("deliver"), 1);
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item)["data"], json!({ "id": "e1" }));
}

pub fn event(id: &str, kind: &str, data: serde_json::Value) -> connectors::ExternalEvent {
    connectors::ExternalEvent {
        id: id.into(),
        connection: ConnectionId::from("c1"),
        capability: Capability::from("pipelines"),
        kind: kind.into(),
        subject: "repo/actions/runs/1".into(),
        data,
        at: jiff::Timestamp::UNIX_EPOCH,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_timer_is_kept_and_fires_on_start_when_it_is_already_due() {
    let world = World::new();
    world.workflow("sleep", &single("sleep", "await ctx.sleep(400); return ctx.pass(ctx.now());"));
    let (kernel, id) = interrupted(&world, "sleep", |item| !item.timers.is_empty()).await;
    let at = kernel.item(&id).expect("store").expect("item").timers.values().next().copied().expect("timer");
    let item = ended(&kernel, &id).await;
    let fired = output(&item)["data"].as_i64().expect("now");
    assert!(fired >= i64::try_from(at).unwrap_or(i64::MAX), "{fired} {at}");
    assert!(item.timers.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_poll_starts_polling_again_after_a_restart() {
    let world = World::new();
    world.provider.then("pipelines.run", Reply::Answer(json!({ "state": "running" })));
    world.provider.then("pipelines.run", Reply::Answer(json!({ "state": "running" })));
    world.provider.always("pipelines.run", json!({ "state": "passed", "ref": "r1" }));
    let body = "const r = await ctx.poll({ capability: \"pipelines\", action: \"run\", args: { run: \"r1\" } }, 20, { until: { state: [\"passed\", \"failed\"] } }); return ctx.pass(r);";
    world.workflow("poll", &single("poll", body));
    let polled = |_: &WorkItem| world.provider.count("pipelines.run") >= 1;
    let (kernel, id) = interrupted(&world, "poll", polled).await;
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item)["data"]["state"], "passed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_child_waiting_on_a_human_survives_and_its_parent_wakes() {
    let world = World::new();
    world.workflow("parent", "import { workflow } from \"sdk\";\nexport default workflow({ name: \"parent\", async run(ctx) { return await ctx.child(\"kid\", { a: 1 }); } });");
    world.workflow("kid", "import { workflow } from \"sdk\";\nexport default workflow({ name: \"kid\", async run(ctx) { const a = await ctx.waitHuman([]); return a.action; } });");
    let (kernel, id) = interrupted(&world, "parent", |item| !item.children.is_empty()).await;
    let child = kernel.item(&id).expect("store").expect("item").children[0].id.clone();
    until(&kernel, &child, |kid| kid.pending_effects().count() == 1).await;
    kernel.human_input(&child, "", "go", json!({})).expect("answer");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!("go"));
    assert!(item.children[0].finished);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pending_ask_survives_a_restart_and_its_answer_is_kept_in_the_log() {
    let world = World::new();
    world.engine.state.ask.store(true, std::sync::atomic::Ordering::SeqCst);
    world.workflow("ask", &single("ask", "return ctx.pass((await ctx.agent(\"worker\", \"planeje\")).text);"));
    let kernel = world.start().await;
    let id = kernel.create_item("ask", json!({ "title": "t" })).expect("item");
    for _ in 0..200 {
        if !kernel.asks().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(kernel.asks()[0].ask.id.0, "ask-1");
    crash(kernel).await;
    let kernel = world.start().await;
    let restored = kernel.asks();
    assert_eq!(restored.len(), 1, "{restored:?}");
    assert_eq!(restored[0].item, id);
    assert_eq!(restored[0].session.as_deref(), Some("s1"));
    tokio::time::sleep(Duration::from_millis(100)).await;
    let current = kernel.asks();
    assert_eq!(current.len(), 1, "the question asked again replaces the restored one: {current:?}");
    let note = json!({ "id": current[0].ask.id.0, "at": "2026-01-01T00:00:00Z", "resolution": "approved" });
    let decision = domain::Decision::Approve(domain::PermissionMode::Bypass);
    kernel.answer_ask_with(&current[0].ask.id, &decision, note.clone()).expect("answer");
    ended(&kernel, &id).await;
    crash(kernel).await;
    let kernel = world.start().await;
    assert!(kernel.asks().is_empty());
    assert_eq!(kernel.answered("s1").expect("answered"), [note]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agent_result_carries_the_plan_put_up_for_approval() {
    let world = World::new();
    world.engine.state.ask.store(true, std::sync::atomic::Ordering::SeqCst);
    world.workflow("plan", &single("plan", "return ctx.pass((await ctx.agent(\"worker\", \"planeje\")).plan);"));
    let kernel = world.start().await;
    let id = kernel.create_item("plan", json!({ "title": "t" })).expect("item");
    for _ in 0..200 {
        if !kernel.asks().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let ask = kernel.asks()[0].ask.id.clone();
    let decision = domain::Decision::Approve(domain::PermissionMode::Plan);
    kernel.answer_ask_with(&ask, &decision, json!({ "resolution": "approved" })).expect("answer");
    assert_eq!(output(&ended(&kernel, &id).await)["data"], json!("o plano"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_given_before_the_resumed_agent_asks_again_is_applied_to_the_new_question() {
    let world = World::new();
    world.engine.state.ask.store(true, std::sync::atomic::Ordering::SeqCst);
    world.workflow("ask", &single("ask", "return ctx.pass((await ctx.agent(\"worker\", \"planeje\")).text);"));
    let kernel = world.start().await;
    let id = kernel.create_item("ask", json!({ "title": "t" })).expect("item");
    for _ in 0..200 {
        if !kernel.asks().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    crash(kernel).await;
    let kernel = world.start().await;
    let restored = kernel.asks();
    assert_eq!(restored.len(), 1);
    let decision = domain::Decision::Approve(domain::PermissionMode::Bypass);
    kernel.answer_ask(&restored[0].ask.id, &decision).expect("answer");
    assert!(kernel.asks().is_empty());
    ended(&kernel, &id).await;
    let answers = world.engine.state.answers.lock().expect("lock").clone();
    assert!(answers.iter().any(|(ask, _)| ask.0 != "ask-1"), "the new question got the early answer: {answers:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_goes_on_with_the_workflow_code_it_started_with_after_the_code_changes() {
    let world = World::new();
    let before = "await ctx.sleep(1); const r = await ctx.waitHuman([]); return ctx.pass({ by: \"antigo\", r });";
    world.workflow("pinned", &single("pinned", before));
    let kernel = world.start().await;
    let id = kernel.create_item("pinned", json!({ "title": "t" })).expect("item");
    until(&kernel, &id, |item| item.steps.iter().any(|run| run.state == StepState::WaitingHuman)).await;
    crash(kernel).await;
    let after = "const r = await ctx.call(\"tracker\", \"issue\", { issue: \"K\" }); return ctx.pass({ by: \"novo\", r });";
    world.workflow("pinned", &single("pinned", after));
    let kernel = world.start().await;
    let waiting = until(&kernel, &id, |item| item.steps.iter().any(|run| run.state == StepState::WaitingHuman)).await;
    assert!(waiting.impediment.is_none(), "the history still replays: {:?}", waiting.impediment);
    let ask = waiting.effects.values().find(|entry| matches!(entry.request.effect, Effect::WaitHuman { .. })).expect("ask");
    kernel.human_input(&id, &ask.request.key.step, "ok", json!({})).expect("answer");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item)["data"]["by"], "antigo");
    let fresh = kernel.create_item("pinned", json!({ "title": "t" })).expect("new item");
    let made = ended(&kernel, &fresh).await;
    assert_eq!(output(&made)["data"]["by"], "novo", "a new item takes the new code");
}
