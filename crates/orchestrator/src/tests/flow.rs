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
    let shown = request.prompt.shown.clone().expect("the workflow's prompt is not shown as the user's");
    assert_eq!((shown.text.as_str(), shown.from.as_deref()), ("faça x", Some("plain")));
    assert_eq!(request.policy.permission, domain::PermissionMode::Auto);
    assert_eq!(request.session.append_system, None);
    let agent = request.session.agent.clone().expect("the step's agent runs as the main agent");
    assert_eq!(agent.name, "worker");
    assert_eq!(agent.definition.as_deref(), Some(WORKER), "its tools, skills and hooks travel with it");
    assert_ne!(request.host.credentials.client, 0, "each agent run has a client of its own");
    let branch = item.branch.clone().expect("the item names its branch before the first agent runs");
    let id_tail: String = id.as_str().chars().filter(char::is_ascii_alphanumeric).collect();
    assert!(branch.starts_with(&format!("{}/", brand::workflow::BRANCH_PREFIX)), "{branch}");
    assert!(branch.ends_with(&id_tail[id_tail.len() - 6..]), "{branch}");
    assert!(request.session.cwd.ends_with(git::folder_for(&branch)));
    let keys = world.provider.keys("tracker.issue");
    assert_eq!(keys.len(), 1);
    assert!(keys[0].starts_with(&format!("{}:{id}/", brand::connectors::MARKER)));
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

#[tokio::test(flavor = "multi_thread")]
async fn the_language_chosen_when_the_item_is_created_reaches_each_agent_request() {
    let world = World::new();
    world.workflow("plain", PLAIN);
    world.store().set_branding("language", &json!("es")).expect("language");
    let kernel = world.start().await;
    let id = kernel.create_item("plain", json!({ "title": "x" })).expect("item");
    ended(&kernel, &id).await;
    let item = kernel.item(&id).expect("store").expect("item");
    assert_eq!(item.input["language"], "es", "the item keeps the language it was created in");
    assert_eq!(world.engine.requests()[0].session.language, Some(domain::Language::Spanish));
    world.store().set_branding("language", &json!("en")).expect("language");
    let other = kernel.create_item("plain", json!({ "title": "y", "language": "pt-BR" })).expect("item");
    ended(&kernel, &other).await;
    assert_eq!(world.engine.requests()[1].session.language, Some(domain::Language::Portuguese));
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
async fn an_unchanged_step_is_viewed_again_without_running_the_script() {
    let world = World::new();
    world.workflow("mine", OVERRIDE);
    world.write("sdk/steps/checks.js", CLIENT_CHECKS);
    let kernel = world.start().await;
    let id = kernel.create_item("mine", json!({})).expect("item");
    ended(&kernel, &id).await;
    let started = std::time::Instant::now();
    let first = kernel.step_view(&id, "checks", None).await.expect("view");
    let ran = started.elapsed();
    let started = std::time::Instant::now();
    let again = kernel.step_view(&id, "checks", None).await.expect("view");
    let kept = started.elapsed();
    eprintln!("step view: ran the script in {ran:?}, served again in {kept:?}");
    assert_eq!(first.blocks, again.blocks);
    let script = kernel.shared.script("mine").expect("script");
    assert_eq!(script.views_kept(), 1);
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
    assert_eq!(steps, Some(13));
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
        agent_engine::tools::clients::waiting(client, 0),
        agent_engine::tools::clients::Waiting::Idle,
        "the client is released when the step ends"
    );
}

const ASKING_TEAM: &str = r#"
import { workflow } from "sdk";

export default workflow({
  name: "asking",
  async run(ctx, item) {
    const done = await ctx.agent("worker", "a equipe pede permissão");
    return { agent: done.text };
  },
});
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_teammate_s_question_waits_on_the_item_and_the_answer_goes_back_to_the_teammate() {
    let world = World::new();
    world.workflow("asking", ASKING_TEAM);
    let kernel = world.start().await;
    let id = kernel.create_item("asking", json!({ "title": "x" })).expect("item");
    let mut asked = Vec::new();
    for _ in 0..500 {
        asked = kernel.asks();
        if !asked.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let pending = asked.first().cloned().expect("the teammate's question reached the item");
    assert_eq!(pending.item, id);
    assert_eq!(pending.ask.id.0, "teammate-tester-r1");
    assert!(matches!(&pending.ask.kind, domain::AskKind::Permission { tool, .. } if tool == "Bash"));
    kernel.answer_ask(&pending.ask.id, &domain::Decision::Allow).expect("answer");
    let cwd = world.engine.requests()[0].session.cwd.clone();
    let team = agent_engine::team::Team::named(&cwd.join(".equipe"), "t");
    let answers = team.unread("tester");
    assert!(answers.iter().any(|mail| mail.text.contains("r1")), "{answers:?}");
    assert!(kernel.asks().is_empty(), "answered once, not asked again");
    team.left("tester");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!({ "agent": "feito 1" }));
}

const CALLING: &str = r#"
import { workflow, step } from "sdk";
const busca = step({
  name: "busca",
  async run(ctx) {
    await ctx.call("tracker", "comment", { issue: "K-1", body: "oi" });
    return ctx.pass(await ctx.call("tracker", "issue", { issue: "K-2" }));
  },
});
export default workflow({ name: "calling", steps: [busca()] });
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_running_step_shows_its_connector_calls_done_and_in_progress() {
    let world = World::new();
    world.workflow("calling", CALLING);
    world.provider.then("tracker.issue", Reply::Hang);
    let kernel = world.start().await;
    let id = kernel.create_item("calling", json!({})).expect("item");
    until(&kernel, &id, |item| item.effects.len() == 2).await;
    let view = kernel.step_view(&id, "busca", None).await.expect("view");
    let calls = view.blocks[0]["calls"].as_array().cloned().unwrap_or_default();
    assert_eq!(view.blocks[0]["type"], "calls", "{}", view.blocks);
    let states: Vec<(&str, &str)> =
        calls.iter().filter_map(|call| Some((call["title"].as_str()?, call["state"].as_str()?))).collect();
    assert_eq!(states, [("comment · K-1", "ok"), ("issue · K-2", "running")]);
    assert_eq!(calls[0]["tag"], "tracker");
    assert!(calls[0]["output"].is_string() && calls[1].get("output").is_none(), "{calls:?}");
}

const PNG: &str = "iVBORw0KGgogcmVzdA==";

const DELIVERED: &str = r#"
import { workflow } from "sdk";
import { plan, develop } from "sdk/steps";
export default workflow({ name: "delivered", steps: [plan(), develop()] });
"#;

#[tokio::test(flavor = "multi_thread")]
async fn the_developer_sees_the_designs_the_plan_read() {
    let world = World::new();
    world.workflow("delivered", DELIVERED);
    let link = "https://www.figma.com/design/AbC/App?node-id=1-2";
    world.provider.always(
        "tracker.issue",
        json!({ "ref": "FIRP-1", "title": "Tooltip", "description": format!("Tela: {link}") }),
    );
    let screen = json!({ "ref": "AbC:1:2", "name": "Cadastro", "file": "App", "url": link, "texts": ["Informe o CPF"],
        "outline": "- Cadastro (frame)", "image": null, "picture": { "media_type": "image/png", "data": PNG } });
    world.provider.always("design.frame", screen);
    let kernel = world.start().await;
    let id = kernel.create_item("delivered", json!({ "title": "FIRP-1", "issue": "FIRP-1" })).expect("item");
    for _ in 0..200 {
        if world.engine.requests().len() >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let requests = world.engine.requests();
    let Some(developer) = requests.get(1) else { panic!("the developer never ran: {} requests", requests.len()) };
    assert!(developer.prompt.text.contains("Implement the item"), "{}", developer.prompt.text);
    assert!(developer.prompt.text.contains("## Design: Cadastro (App)"), "{}", developer.prompt.text);
    assert!(developer.prompt.text.contains("- Informe o CPF"), "{}", developer.prompt.text);
    assert_eq!(developer.prompt.images.len(), 1);
    drop(id);
}

const PLANNED: &str = r#"
import { workflow } from "sdk";
import { plan } from "sdk/steps";
export default workflow({ name: "planned", steps: [plan()] });
"#;

#[tokio::test(flavor = "multi_thread")]
async fn planning_reads_the_issue_from_the_tracker_and_hands_it_to_the_agent() {
    let world = World::new();
    world.workflow("planned", PLANNED);
    let issue = json!({ "ref": "FIRP-2532", "title": "Exportar extrato", "status": "A fazer", "url": "https://t/FIRP-2532",
        "type": "História", "description": "Como cliente quero exportar o extrato.\n\nCritérios: CSV e PDF.\n\nProtótipo: https://www.figma.com/design/AbC/App?node-id=1-2.",
        "subtasks": [{ "ref": "FIRP-2533", "title": "Backend", "status": "A fazer" }],
        "comments": [{ "author": { "name": "Ana" }, "body": "Só o mês corrente.", "at": "2026-11-20" }] });
    world.provider.always("tracker.issue", issue);
    let screen = json!({ "ref": "AbC:1:2", "name": "Extrato", "file": "App", "url": "https://www.figma.com/design/AbC/App?node-id=1-2",
        "texts": ["Exportar extrato"], "outline": "- Extrato (frame)", "image": "https://img/1-2.png",
        "picture": { "media_type": "image/png", "data": PNG } });
    world.provider.always("design.frame", screen);
    let kernel = world.start().await;
    let id = kernel.create_item("planned", json!({ "title": "FIRP-2532", "issue": "FIRP-2532" })).expect("item");
    for _ in 0..100 {
        if !world.engine.requests().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let text = world.engine.requests().first().map(|request| request.prompt.text.clone()).unwrap_or_default();
    for expected in [
        "Issue FIRP-2532: Exportar extrato",
        "Critérios: CSV e PDF.",
        "FIRP-2533 Backend",
        "Só o mês corrente.",
        "already read for you",
        "## Design: Extrato (App)",
        "- Exportar extrato",
        "attached as images",
    ] {
        assert!(text.contains(expected), "{expected} missing in {text}");
    }
    let images = world.engine.requests().first().map(|request| request.prompt.images.clone()).unwrap_or_default();
    assert_eq!(images.iter().map(|image| image.media_type.as_str()).collect::<Vec<_>>(), ["image/png"]);
    let asked = world
        .provider
        .calls
        .lock()
        .expect("lock")
        .iter()
        .any(|(action, args, _)| action == "tracker.issue" && args["issue"] == "FIRP-2532");
    assert!(asked, "the plan step reads the issue");
    let designed = world.provider.calls.lock().expect("lock").iter().any(|(action, args, _)| {
        action == "design.frame" && args["url"] == "https://www.figma.com/design/AbC/App?node-id=1-2"
    });
    assert!(designed, "the plan step reads the linked design");
    drop(id);
}

#[test]
fn only_real_images_from_the_input_reach_the_agent() {
    let input = json!({ "images": [
        { "media_type": "image/png", "data": PNG },
        { "media_type": "image/jpeg", "data": PNG },
        { "media_type": "image/png", "data": "!!" },
        { "media_type": "image/png" },
        "solto"
    ] });
    let images = crate::agent::images_of(&input);
    assert_eq!(images.len(), 1);
    assert!(crate::agent::images_of(&json!({ "prompt": "x" })).is_empty());
}
