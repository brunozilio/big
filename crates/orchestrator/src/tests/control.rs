use std::time::Duration;

use domain::workflow::StepState;
use serde_json::json;

use super::*;

const TRY: &str = r#"
import { workflow } from "sdk";
export default workflow({
  name: "try",
  async run(ctx) {
    try {
      const r = await ctx.call("tracker", "issue", { issue: "K" });
      return { ok: r.n };
    } catch (error) {
      return { failed: error.kind };
    }
  },
});
"#;

async fn tried(world: &World) -> serde_json::Value {
    world.workflow("try", TRY);
    let kernel = world.start().await;
    let id = kernel.create_item("try", json!({})).expect("item");
    output(&ended(&kernel, &id).await)
}

#[tokio::test(flavor = "multi_thread")]
async fn transient_errors_and_rate_limits_are_retried_and_permanent_ones_reach_the_workflow() {
    let world = World::new();
    world.provider.then("tracker.issue", Reply::Fail(ActionError::transient("503")));
    world.provider.then("tracker.issue", Reply::Fail(ActionError::rate_limited(Duration::from_millis(5), "429")));
    assert_eq!(tried(&world).await, json!({ "ok": 3 }));
    let keys = world.provider.keys("tracker.issue");
    assert!(keys.iter().all(|key| key == &keys[0]), "every try keeps the key");
    let world = World::new();
    world.provider.then("tracker.issue", Reply::Fail(ActionError::permanent("404")));
    assert_eq!(tried(&world).await, json!({ "failed": "permanent" }));
    assert_eq!(world.provider.count("tracker.issue"), 1);
    let world = World::new();
    for _ in 0..4 {
        world.provider.then("tracker.issue", Reply::Fail(ActionError::transient("503")));
    }
    assert_eq!(tried(&world).await, json!({ "failed": "transient" }));
    assert_eq!(world.provider.count("tracker.issue"), 4, "the policy allows four tries");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_auth_error_waits_for_a_human_to_reconnect_and_a_rerun_tries_again() {
    let world = World::new();
    world.workflow("try", TRY);
    world.provider.then("tracker.issue", Reply::Fail(ActionError::reconnect()));
    let kernel = world.start().await;
    let id = kernel.create_item("try", json!({})).expect("item");
    let blocked = until(&kernel, &id, |item| item.impediment.is_some()).await;
    let reason = blocked.impediment.and_then(|found| found.reason).unwrap_or_default();
    assert!(reason.starts_with("reconecte"), "{reason}");
    assert_eq!(kernel.items().expect("items")[0].group, domain::board::Group::WithYou);
    kernel.rerun(&id).await.expect("rerun");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!({ "ok": 2 }));
    assert!(item.impediment.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_the_connection_cannot_make_yet_becomes_an_impediment_with_its_reason() {
    let world = World::new();
    world.workflow("try", TRY);
    world.provider.then("tracker.issue", Reply::Fail(ActionError::blocked("Escolha o projeto.")));
    let kernel = world.start().await;
    let id = kernel.create_item("try", json!({})).expect("item");
    let blocked = until(&kernel, &id, |item| item.impediment.is_some()).await;
    assert_eq!(blocked.impediment.and_then(|found| found.reason).as_deref(), Some("Escolha o projeto."));
    assert_eq!(world.provider.count("tracker.issue"), 1, "not retried");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_blocked_call_does_not_time_out_while_it_waits_and_runs_again_with_a_fresh_timeout() {
    let world = World::new();
    let source = TRY.replace("{ issue: \"K\" })", "{ issue: \"K\" }, { timeout: 300 })");
    world.workflow("try", &source);
    world.provider.then("tracker.issue", Reply::Fail(ActionError::blocked("Escolha o projeto.")));
    let kernel = world.start().await;
    let id = kernel.create_item("try", json!({})).expect("item");
    until(&kernel, &id, |item| item.impediment.is_some()).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    let waiting = kernel.item(&id).expect("store").expect("item");
    assert!(!waiting.is_ended(), "the blocked call timed out: {:?}", waiting.ending);
    assert!(waiting.effects.values().all(|entry| entry.outcome.is_none()), "nothing resolved while blocked");
    assert!(waiting.timers.is_empty(), "the timeout was cancelled: {:?}", waiting.timers);
    kernel.rerun(&id).await.expect("rerun");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!({ "ok": 2 }));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_without_a_connection_becomes_an_impediment() {
    let world = World::new();
    world.workflow(
        "nothing",
        "import { workflow } from \"sdk\";\nexport default workflow({ name: \"nothing\", async run(ctx) { return await ctx.call(\"cloud\", \"identity\", {}); } });",
    );
    let kernel = world.start().await;
    let id = kernel.create_item("nothing", json!({})).expect("item");
    let item = until(&kernel, &id, |item| item.impediment.is_some()).await;
    assert_eq!(item.impediment.and_then(|found| found.reason).as_deref(), Some("connect an account for cloud"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_optional_call_without_a_connection_resolves_to_null() {
    let world = World::new();
    world.workflow(
        "maybe",
        "import { workflow } from \"sdk\";\nexport default workflow({ name: \"maybe\", async run(ctx) { const r = await ctx.call(\"cloud\", \"identity\", {}, { optional: true }); return { got: r }; } });",
    );
    let kernel = world.start().await;
    let id = kernel.create_item("maybe", json!({})).expect("item");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!({ "got": null }));
    assert!(item.impediment.is_none());
}

const STEPS: &str = r#"
import { workflow, step } from "sdk";
const prepare = step({ name: "prepare", async run(ctx) { await ctx.exec("true"); return ctx.pass(); } });
const build = step({
  name: "build",
  async run(ctx) {
    const r = await ctx.agent("worker", "construa", { worktree: ctx.item.branch });
    return ctx.pass({ session: r.session, commit: r.commit });
  },
});
export default workflow({ name: "steps", statuses: ["Fazendo"], steps: [prepare({ status: "Fazendo" }), build()] });
"#;

#[tokio::test(flavor = "multi_thread")]
async fn rewinding_a_step_restores_the_worktree_and_the_session_and_runs_it_again() {
    let mut world = World::new();
    world.init_git();
    world.workflow("steps", STEPS);
    *world.engine.state.write.lock().expect("lock") = Some("feito.txt".into());
    let kernel = world.start().await;
    let id = kernel.create_item("steps", json!({})).expect("item");
    let first = ended(&kernel, &id).await;
    assert_eq!(first.status.as_deref(), Some("Fazendo"));
    let branch = git::head_of(&first, "");
    let worktree = world.dir.path().join("data/worktrees").join(git::folder_for(&branch));
    let mut waited = 0;
    while worktree.exists() && waited < 100 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        waited += 1;
    }
    assert!(!worktree.exists(), "an ended item gives its clean worktree back");
    *world.engine.state.write.lock().expect("lock") = None;
    kernel.rewind(&id, "build").await.expect("rewind");
    let again = until(&kernel, &id, |item| item.is_ended() && item.version > first.version).await;
    assert!(!worktree.join("feito.txt").exists(), "the commit of the step was undone");
    let requests = world.engine.requests();
    let rewound = requests.iter().find_map(|request| request.session.command);
    assert_eq!(rewound, Some(domain::SessionCommand::Rewind(1)));
    assert_eq!(world.engine.runs(), 2);
    assert_eq!(output(&again)["data"]["session"], "s3");
    let build = again.step("build", Some("main")).expect("build");
    assert_eq!((build.state, build.attempt), (StepState::Passed, 2));
    assert_eq!(again.step("prepare", Some("main")).map(|run| run.attempt), Some(1), "before the cut stays");
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_a_parent_cancels_its_waiting_child() {
    let world = World::new();
    world.workflow("parent", "import { workflow } from \"sdk\";\nexport default workflow({ name: \"parent\", async run(ctx) { return await ctx.child(\"kid\", {}); } });");
    world.workflow("kid", "import { workflow } from \"sdk\";\nexport default workflow({ name: \"kid\", async run(ctx) { return await ctx.waitHuman([]); } });");
    let kernel = world.start().await;
    let id = kernel.create_item("parent", json!({})).expect("item");
    let item = until(&kernel, &id, |item| !item.children.is_empty()).await;
    let child = item.children[0].id.clone();
    until(&kernel, &child, |kid| kid.pending_effects().count() == 1).await;
    kernel.cancel(&id).expect("cancel");
    assert!(ended(&kernel, &id).await.is_cancelled());
    assert!(ended(&kernel, &child).await.is_cancelled());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_question_reaches_the_human_and_the_answer_reaches_the_agent() {
    let world = World::new();
    world.engine.state.ask.store(true, Ordering::SeqCst);
    world.workflow("ask", "import { workflow } from \"sdk\";\nexport default workflow({ name: \"ask\", async run(ctx) { return (await ctx.agent(\"worker\", \"planeje\")).text; } });");
    let kernel = world.start().await;
    let mut events = kernel.subscribe();
    let id = kernel.create_item("ask", json!({})).expect("item");
    let mut asked = None;
    for _ in 0..200 {
        if let Some(found) = kernel.asks().first() {
            asked = Some(found.clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let asked = asked.expect("the question reached the human");
    assert_eq!(asked.item, id);
    assert_eq!(asked.session.as_deref(), Some("s1"));
    assert_eq!(kernel.items().expect("items")[0].group, domain::board::Group::WithYou);
    kernel.answer_ask(&asked.ask.id, &domain::Decision::Approve(domain::PermissionMode::Bypass)).expect("answer");
    assert_eq!(output(&ended(&kernel, &id).await), json!("feito 1"));
    assert!(kernel.asks().is_empty());
    assert_eq!(world.engine.state.answers.lock().expect("lock").len(), 1);
    let mut saw_session = false;
    while let Ok(event) = events.try_recv() {
        saw_session |= matches!(event, crate::OrchestratorEvent::Session { .. });
    }
    assert!(saw_session);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_whose_agent_waits_for_the_human_is_not_shown_as_running() {
    use domain::workflow::StepState;
    let world = World::new();
    world.engine.state.ask.store(true, Ordering::SeqCst);
    let body = "return ctx.pass((await ctx.agent(\"worker\", \"faça\")).text);";
    let source = format!(
        "import {{ workflow, step }} from \"sdk\";\nconst only = step({{ name: \"only\", async run(ctx) {{ {body} }} }});\nexport default workflow({{ name: \"waits\", steps: [only()] }});\n"
    );
    world.workflow("waits", &source);
    let kernel = world.start().await;
    let id = kernel.create_item("waits", json!({})).expect("item");
    let mut asked = None;
    for _ in 0..200 {
        if let Some(found) = kernel.asks().first() {
            asked = Some(found.clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let asked = asked.expect("the question reached the human");
    let summary = kernel.summary(&id).expect("summary").expect("item");
    let shown = summary.steps.iter().filter(|step| step.id == "only").map(|step| step.state).collect::<Vec<_>>();
    assert_eq!(shown, vec![StepState::WaitingHuman]);
    let view = kernel.step_view(&id, "only", None).await.expect("view");
    assert_eq!(view.attempts.last().map(|attempt| attempt.state), Some(StepState::WaitingHuman));
    kernel.answer_ask(&asked.ask.id, &domain::Decision::Approve(domain::PermissionMode::Bypass)).expect("answer");
    ended(&kernel, &id).await;
    let after = kernel.summary(&id).expect("summary").expect("item");
    assert!(after.steps.iter().all(|step| step.state != StepState::WaitingHuman));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approved_plan_ends_a_planning_agent_even_if_it_would_keep_going() {
    let world = World::new();
    world.engine.state.ask.store(true, Ordering::SeqCst);
    world.engine.state.keeps_going.store(true, Ordering::SeqCst);
    world.workflow("plano", "import { workflow } from \"sdk\";\nexport default workflow({ name: \"plano\", async run(ctx) { return (await ctx.agent(\"worker\", \"planeje\", { mode: \"plan\" })).plan; } });");
    let kernel = world.start().await;
    let id = kernel.create_item("plano", json!({})).expect("item");
    let mut asked = None;
    for _ in 0..200 {
        if let Some(found) = kernel.asks().first() {
            asked = Some(found.clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let asked = asked.expect("the plan reached the human");
    kernel.answer_ask(&asked.ask.id, &domain::Decision::Approve(domain::PermissionMode::AcceptEdits)).expect("answer");
    assert_eq!(output(&ended(&kernel, &id).await), json!("o plano"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_status_is_kept_and_sent_to_the_tracker_of_the_item_s_issue() {
    let world = World::new();
    world.workflow("status", "import { workflow } from \"sdk\";\nexport default workflow({ name: \"status\", async run(ctx) { await ctx.status(\"Fazendo\"); await ctx.impediment(true, \"travou\"); return null; } });");
    let kernel = world.start().await;
    let id = kernel.create_item("status", json!({ "issue": "K-7" })).expect("item");
    let item = ended(&kernel, &id).await;
    assert_eq!(item.status.as_deref(), Some("Fazendo"));
    let calls = world.provider.calls.lock().expect("lock").clone();
    let transition = calls.iter().find(|(action, _, _)| action == "tracker.transition").expect("transition");
    assert_eq!(transition.1, json!({ "issue": "K-7", "status": "Fazendo" }));
    assert!(calls.iter().any(|(action, args, _)| action == "tracker.set_impediment" && args["reason"] == "travou"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_status_the_tracker_board_does_not_have_does_not_stop_the_item() {
    let world = World::new();
    world
        .provider
        .then("tracker.transition", Reply::Fail(ActionError::permanent("K-7 cannot go from Fazendo to Revisando")));
    world.workflow("status", "import { workflow } from \"sdk\";\nexport default workflow({ name: \"status\", async run(ctx) { await ctx.status(\"Revisando\"); return \"seguiu\"; } });");
    let kernel = world.start().await;
    let id = kernel.create_item("status", json!({ "issue": "K-7" })).expect("item");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!("seguiu"));
    assert_eq!(item.status.as_deref(), Some("Revisando"), "the item keeps its own status");
    assert!(item.impediment.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_item_gets_a_short_name_from_its_request() {
    let world = World::new();
    world.workflow("hold", "import { workflow } from \"sdk\";\nexport default workflow({ name: \"hold\", async run(ctx) { return await ctx.waitHuman([]); } });");
    let kernel = world.start().await;
    let text = "Criar um e-commerce básico para a nossa loja com catálogo, carrinho e checkout";
    let id = kernel.create_item("hold", json!({ "title": text, "text": text })).expect("item");
    let named = until(&kernel, &id, |item| item.name.is_some()).await;
    assert_eq!(named.name.as_deref(), Some("Loja virtual"));
    let asked: Vec<String> = world.engine.titled().into_iter().map(|request| request.prompt.text).collect();
    assert_eq!(asked, [text.to_owned()]);
    let summary = kernel.summary(&id).expect("summary").expect("item");
    assert_eq!(summary.title, "Loja virtual");
}

#[tokio::test(flavor = "multi_thread")]
async fn at_most_the_configured_number_of_items_work_at_once() {
    let mut world = World::new();
    world.parallel = 1;
    world.provider.then("tracker.issue", Reply::Hang);
    world.workflow("try", TRY);
    let kernel = world.start().await;
    let first = kernel.create_item("try", json!({})).expect("item");
    let second = kernel.create_item("try", json!({})).expect("item");
    until(&kernel, &first, |item| item.effects.len() == 1).await;
    until(&kernel, &second, |item| item.effects.len() == 1).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(world.provider.count("tracker.issue"), 1, "the second item waits for the only permit");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trigger_creates_one_item_per_subject_and_the_sync_reports_health() {
    let mut world = World::new();
    world.sync = true;
    world.workflow(
        "intake",
        "import { workflow, on } from \"sdk\";\nexport default workflow({ name: \"intake\", triggers: [on(\"tracker.issue.created\", { status: \"A fazer\" })], async run(ctx) { return await ctx.waitHuman([]); } });",
    );
    let kernel = world.start().await;
    assert!(kernel.health().is_some());
    let mut created =
        super::restart::event("t1", "tracker.issue.created", json!({ "status": "A fazer", "title": "Nova" }));
    created.subject = "K-9".into();
    assert_eq!(kernel.trigger(&created).expect("trigger"), 1);
    assert_eq!(kernel.trigger(&created).expect("trigger"), 0, "one item per subject");
    let mut other = created.clone();
    other.data = json!({ "status": "Feito" });
    other.subject = "K-10".into();
    assert_eq!(kernel.trigger(&other).expect("trigger"), 1, "the built-in delivery takes what the filter left out");
    let all = kernel.items().expect("items");
    assert!(all.iter().any(|item| item.workflow == "default" && item.issue.as_deref() == Some("K-10")));
    let items: Vec<_> = all.into_iter().filter(|item| item.workflow == "intake").collect();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Nova");
    assert_eq!(items[0].issue.as_deref(), Some("K-9"));
    let kept = kernel.item(&items[0].id).expect("store").expect("item");
    assert_eq!(kept.input["sources"], json!([{ "kind": "issue", "connection": "c1", "subject": "K-9" }]));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_trigger_keeps_a_typed_source_and_no_issue() {
    let world = World::new();
    world.workflow(
        "reviews",
        "import { workflow, on } from \"sdk\";\nexport default workflow({ name: \"reviews\", triggers: [on(\"code.pr.opened\")], async run(ctx) { return await ctx.waitHuman([]); } });",
    );
    let kernel = world.start().await;
    let mut opened = super::restart::event("p1", "code.pr.opened", json!({ "title": "Ajustar cache" }));
    opened.subject = "o/r#12".into();
    assert_eq!(kernel.trigger(&opened).expect("trigger"), 1);
    assert_eq!(kernel.trigger(&opened).expect("trigger"), 0, "one item per pull request");
    let items = kernel.items().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Ajustar cache");
    assert_eq!(items[0].issue, None);
    let kept = kernel.item(&items[0].id).expect("store").expect("item");
    assert_eq!(kept.input["sources"], json!([{ "kind": "pull_request", "connection": "c1", "subject": "o/r#12" }]));
    assert!(kept.input["issue"].is_null() && kept.input["connection"].is_null());
}

const SKIPS: &str = r#"
import { workflow, step } from "sdk";
const nothing = step({ name: "nothing", async run(ctx) { return ctx.skip({ why: "não há o que fazer" }); } });
const after = step({ name: "after", async run(ctx) { return ctx.pass({ ran: true }); } });
export default workflow({ name: "skips", steps: [nothing(), after()] });
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_step_that_skips_itself_shows_as_skipped_and_the_next_one_runs() {
    let world = World::new();
    world.workflow("skips", SKIPS);
    let kernel = world.start().await;
    let id = kernel.create_item("skips", json!({})).expect("item");
    let item = ended(&kernel, &id).await;
    let skipped = item.step("nothing", Some("main")).expect("nothing");
    assert_eq!(skipped.state, StepState::Skipped);
    assert_eq!(skipped.output["why"], "não há o que fazer");
    assert_eq!(item.step("after", Some("main")).map(|run| run.state), Some(StepState::Passed));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ai_provider_out_of_quota_stops_the_item_with_the_reason_instead_of_failing_the_step() {
    let world = World::new();
    world.engine.state.limited.store(true, Ordering::SeqCst);
    world.workflow("limited", "import { workflow } from \"sdk\";\nexport default workflow({ name: \"limited\", async run(ctx) { return (await ctx.agent(\"worker\", \"faça\")).text; } });");
    let kernel = world.start().await;
    let id = kernel.create_item("limited", json!({})).expect("item");
    let item = until(&kernel, &id, |item| item.impediment.is_some()).await;
    assert!(!item.is_ended());
    let reason = item.impediment.and_then(|found| found.reason).unwrap_or_default();
    assert_eq!(reason, "o provedor de IA atingiu o limite de uso: HTTP 400: You have reached the limit.");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_tried_again_after_a_failure_is_the_current_one() {
    let world = World::new();
    world.provider.then("tracker.issue", Reply::Fail(ActionError::permanent("404")));
    let body = "const r = await ctx.call(\"tracker\", \"issue\", { issue: \"K\" }); await ctx.waitHuman([]); return ctx.pass(r);";
    let source = format!(
        "import {{ workflow, step }} from \"sdk\";\nconst only = step({{ name: \"only\", async run(ctx) {{ {body} }} }});\nexport default workflow({{ name: \"again\", steps: [only()] }});\n"
    );
    world.workflow("again", &source);
    let kernel = world.start().await;
    let id = kernel.create_item("again", json!({})).expect("item");
    until(&kernel, &id, |item| item.step("", Some("main")).is_some_and(|run| run.state == StepState::WaitingHuman))
        .await;
    kernel.human_input(&id, "", "retry", json!({})).expect("retry");
    until(&kernel, &id, |item| {
        item.step("only", Some("main")).is_some_and(|run| run.attempt == 2 && run.state == StepState::WaitingHuman)
    })
    .await;
    let summary = kernel.summary(&id).expect("summary").expect("item");
    assert_eq!(summary.current.as_deref(), Some("only"));
    let view = kernel.step_view(&id, "only", None).await.expect("view");
    assert_eq!(
        view.attempts.last().map(|attempt| attempt.state),
        Some(StepState::WaitingHuman),
        "not shown as running"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_step_the_person_skips_shows_as_skipped() {
    let world = World::new();
    world.provider.then("tracker.issue", Reply::Fail(ActionError::permanent("404")));
    let source = "import { workflow, step } from \"sdk\";\nconst only = step({ name: \"only\", async run(ctx) { return ctx.pass(await ctx.call(\"tracker\", \"issue\", { issue: \"K\" })); } });\nconst after = step({ name: \"after\", async run(ctx) { return ctx.pass(1); } });\nexport default workflow({ name: \"skip\", steps: [only(), after()] });\n";
    world.workflow("skip", source);
    let kernel = world.start().await;
    let id = kernel.create_item("skip", json!({})).expect("item");
    until(&kernel, &id, |item| item.step("", Some("main")).is_some_and(|run| run.state == StepState::WaitingHuman))
        .await;
    kernel.human_input(&id, "", "skip", json!({})).expect("skip");
    let item = ended(&kernel, &id).await;
    assert_eq!(item.step("only", Some("main")).map(|run| run.state), Some(StepState::Skipped));
    assert_eq!(item.step("after", Some("main")).map(|run| run.state), Some(StepState::Passed));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_that_sends_the_item_back_reads_as_not_approved() {
    let world = World::new();
    let source = "import { workflow, step } from \"sdk\";\nconst make = step({ name: \"make\", async run(ctx) { if (ctx.from) await ctx.waitHuman([]); return ctx.pass(1); } });\nconst look = step({ name: \"look\", async run(ctx) { return ctx.goto(\"make\", { body: \"refaça\" }); } });\nexport default workflow({ name: \"back\", steps: [make(), look()] });\n";
    world.workflow("back", source);
    let kernel = world.start().await;
    let id = kernel.create_item("back", json!({})).expect("item");
    let item = until(&kernel, &id, |item| item.step("make", Some("main")).is_some_and(|run| run.attempt == 2)).await;
    assert_eq!(item.step("look", Some("main")).map(|run| run.state), Some(StepState::Pending), "it runs again later");
    let view = kernel.step_view(&id, "look", None).await.expect("view");
    assert_eq!(view.attempts.last().map(|attempt| attempt.state), Some(StepState::Failed));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_counts_as_running_only_while_its_item_s_agent_works() {
    let world = World::new();
    world.engine.state.hold.store(true, Ordering::SeqCst);
    world.workflow("hold", "import { workflow } from \"sdk\";\nexport default workflow({ name: \"hold\", async run(ctx) { return (await ctx.agent(\"worker\", \"faça\")).text; } });");
    let kernel = world.start().await;
    let id = kernel.create_item("hold", json!({})).expect("item");
    until(&kernel, &id, |item| item.effects.values().any(|entry| entry.progress["session"] == "s1")).await;
    assert!(kernel.runs_session("s1").expect("asked"));
    assert!(!kernel.runs_session("another").expect("asked"));
    world.engine.state.hold.store(false, Ordering::SeqCst);
    ended(&kernel, &id).await;
    assert!(!kernel.runs_session("s1").expect("asked"), "the run is over");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_earlier_attempt_shows_its_own_conversation() {
    let world = World::new();
    world.provider.then("tracker.issue", Reply::Fail(ActionError::permanent("404")));
    let body = "await ctx.agent(\"worker\", \"faça\"); return ctx.pass(await ctx.call(\"tracker\", \"issue\", { issue: \"K\" }));";
    let source = format!(
        "import {{ workflow, step }} from \"sdk\";\nconst only = step({{ name: \"only\", async run(ctx) {{ {body} }} }});\nexport default workflow({{ name: \"talks\", steps: [only()] }});\n"
    );
    world.workflow("talks", &source);
    let kernel = world.start().await;
    let id = kernel.create_item("talks", json!({})).expect("item");
    until(&kernel, &id, |item| item.step("", Some("main")).is_some_and(|run| run.state == StepState::WaitingHuman))
        .await;
    kernel.human_input(&id, "", "retry", json!({})).expect("retry");
    ended(&kernel, &id).await;
    let latest = kernel.step_view(&id, "only", None).await.expect("latest");
    let first = kernel.step_view(&id, "only", Some(1)).await.expect("first");
    assert_eq!((latest.attempt, first.attempt), (2, 1));
    assert_ne!(latest.session, first.session);
    assert!(first.session.is_some());
}
