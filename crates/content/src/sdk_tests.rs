use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

struct NoHost;

impl script::Host for NoHost {
    fn call(&self, function: &str, _arguments: Value) -> Result<Value, String> {
        Err(format!("the sdk calls nothing on the host, yet it called {function}"))
    }
}

const FILES: [&str; 9] = [
    "sdk/index.js",
    "sdk/index.d.ts",
    "sdk/PROTOCOL.md",
    "sdk/capabilities/index.d.ts",
    "sdk/capabilities/tracker.d.ts",
    "sdk/capabilities/code.d.ts",
    "sdk/capabilities/pipelines.d.ts",
    "sdk/capabilities/cloud.d.ts",
    "sdk/capabilities/tests.d.ts",
];

const PROVIDERS: [&str; 8] = ["jira", "github", "aws", "atlassian", "gitlab", "bitbucket", "zephyr", "smartbear"];

const ACTIONS: [(&str, &[&str]); 5] = [
    ("tracker", &["create_issue", "issue", "comment", "statuses", "transition", "set_impediment", "search"]),
    (
        "code",
        &[
            "open_pull_request",
            "pull_request",
            "files_diff",
            "review_comments",
            "submit_review",
            "resolve_thread",
            "merge",
        ],
    ),
    ("pipelines", &["runs", "run", "job_log", "pending_deployments", "review_deployment"]),
    ("cloud", &["identity", "accounts_and_roles"]),
    ("tests", &["create_case", "cases", "create_cycle", "record", "projects"]),
];

const DELIVERY: &str = r#"
const checks = step({
  name: "checks",
  view: (s) => [logs(s.output || ""), actions(s.failed ? ["rerun"] : [])],
  async run(ctx, opts) {
    for (const cmd of opts.commands) {
      const r = await ctx.exec(cmd);
      if (r.code !== 0) return ctx.fail({ output: r.output });
    }
    return ctx.pass();
  },
});

workflow({
  name: "entrega",
  triggers: [on("tracker.issue.created", { status: "A fazer" })],
  async run(ctx, item) {
    const plan = await ctx.agent("planner", `planeje ${item.title}`);
    const both = await ctx.all([
      (branch) => checks(branch, { commands: ["lint", "test"] }),
      (branch) => branch.call("tracker", "comment", { issue: item.issue, body: plan.text }, { connection: "tracker" }),
    ]);
    const first = await ctx.race([ctx.event("tracker.status.changed", { to: "Cancelado" }), ctx.sleep(1000)]);
    if (first.kind === "event") return ctx.cancel("cancelado no tracker");
    return { comment: both[1].id, now: ctx.now(), roll: ctx.random() };
  },
});
"#;

const STEPS: &str = r#"
const develop = step({ name: "develop", async run(ctx) { await ctx.agent("developer", "faça"); } });
const checks = step({
  name: "checks",
  async run(ctx, opts) {
    const r = await ctx.exec(opts.command);
    return r.code === 0 ? ctx.pass() : ctx.fail({ output: r.output });
  },
});
workflow({
  name: "lista",
  statuses: ["Fazendo", "Feito"],
  steps: [develop({ status: "Fazendo" }), checks({ command: "test", onFail: "develop", retries: 1 })],
});
"#;

fn module(workflow: &str) -> String {
    format!("{}\n{workflow}", super::builtin_text("sdk/index.js").expect("the runtime is built in"))
}

fn activate(source: &str, activation: &Value) -> Value {
    let host: Arc<dyn script::Host> = Arc::new(NoHost);
    let stop = Arc::new(AtomicBool::new(false));
    script::run("sdk.js", source, "host.activate", std::slice::from_ref(activation), &Value::Null, host, stop)
        .expect("activation")
}

fn answer(command: &Value) -> Value {
    match command["kind"].as_str().expect("kind") {
        "agent" => json!({ "text": "o plano", "session": "s1", "output": null }),
        "exec" => json!({ "code": 0, "output": "ok", "stdout": "ok", "stderr": "" }),
        "call" => json!({ "id": "c9", "author": null, "body": "o plano", "at": "agora" }),
        _ => Value::Null,
    }
}

fn drive(source: &str, target: Value, mut reply: impl FnMut(&Value) -> Option<Value>) -> (Value, Vec<Value>) {
    let mut journal: Vec<Value> = Vec::new();
    for round in 0..40 {
        let activation = json!({ "target": target, "item": { "id": "i1", "title": "x", "issue": "T-1" },
            "journal": journal, "start": 1000, "seed": "i1" });
        let result = activate(source, &activation);
        let again = activate(source, &activation);
        assert_eq!(result, again, "the same journal replays to the same result");
        if result["state"] != "running" {
            return (result, journal);
        }
        let pending = result["pending"].as_array().expect("pending").clone();
        assert!(!pending.is_empty(), "a running workflow waits on something: {result}");
        for command in pending.iter().rev() {
            let Some(value) = reply(command) else { continue };
            journal.push(json!({ "seq": command["seq"], "kind": command["kind"], "ok": value, "at": 2000 + round }));
        }
    }
    panic!("the workflow never ended")
}

#[test]
fn every_sdk_file_is_built_in_and_names_no_provider() {
    for path in FILES {
        let text = super::builtin_text(path).unwrap_or_else(|| panic!("{path} is built in"));
        let lower = text.to_lowercase();
        for provider in PROVIDERS {
            assert!(!lower.contains(provider), "{path} names {provider}");
        }
    }
    for (capability, actions) in ACTIONS {
        let contract = super::builtin_text(&format!("sdk/capabilities/{capability}.d.ts")).expect("contract");
        for action in actions {
            assert!(contract.contains(&format!("  {action}: {{")), "{capability}.{action}");
        }
    }
}

#[test]
fn the_delivery_agents_are_built_in_with_a_header() {
    for (name, color) in
        [("planner", "#e94b9b"), ("developer", "#3b82f6"), ("reviewer", "#4fb39c"), ("release", "#f0a235")]
    {
        let text = super::builtin_text(&format!("agents/{name}.md")).expect("built in");
        let header = text.strip_prefix("---\n").and_then(|rest| rest.split_once("\n---\n")).map(|(head, _)| head);
        let header = header.unwrap_or_else(|| panic!("{name} has a header"));
        for key in ["name:", "label:", "description:", "tools:", "permissionMode:", "model: auto", "skills:", "avatar:"]
        {
            assert!(header.contains(key), "{name} lacks {key}");
        }
        assert!(header.contains(color), "{name} keeps its color");
    }
}

#[test]
fn the_runtime_loads_and_describes_what_it_registered() {
    let host: Arc<dyn script::Host> = Arc::new(NoHost);
    let stop = Arc::new(AtomicBool::new(false));
    let described =
        script::run("sdk.js", &module(DELIVERY), "host.describe", &[], &Value::Null, host, stop).expect("describe");
    assert_eq!(described["workflows"][0]["name"], "entrega");
    assert_eq!(described["workflows"][0]["code"], true);
    assert_eq!(described["workflows"][0]["triggers"][0]["kind"], "tracker.issue.created");
    assert_eq!(described["steps"], json!(["checks"]));
    assert_eq!(script::exported("sdk.js", &module(""), "PROTOCOL").expect("protocol"), json!(1));
}

#[test]
fn a_workflow_runs_by_replay_with_stable_branches_and_ends() {
    let source = module(DELIVERY);
    let mut asked = Vec::new();
    let (result, journal) = drive(&source, json!({ "workflow": "entrega" }), |command| {
        asked.push(command.clone());
        (command["kind"] != "event").then(|| answer(command))
    });
    assert_eq!(result["state"], "completed", "{result}");
    assert_eq!(result["result"]["comment"], "c9");
    assert_eq!(result["result"]["now"], journal.last().expect("journal")["at"]);
    assert!(result["result"]["roll"].as_f64().is_some_and(|roll| (0.0..1.0).contains(&roll)));
    let event = asked.iter().find(|command| command["kind"] == "event").expect("an event wait");
    assert_eq!(result["cancel"], json!([event["seq"]]), "the losing branch of the race is cancelled");
    let fork = result["signals"].as_array().expect("signals").iter().find(|signal| signal["kind"] == "fork").cloned();
    let branches = fork.expect("a fork")["branches"].clone();
    let comment = asked.iter().find(|command| command["kind"] == "call").expect("a call");
    assert_eq!(comment["branch"], branches[1]);
    assert_eq!(comment["connection"], "tracker");
    let execs: Vec<&Value> = asked.iter().filter(|command| command["kind"] == "exec").collect();
    assert!(execs.iter().all(|command| command["branch"] == branches[0] && command["step"] == "checks"));
}

#[test]
fn a_changed_workflow_is_caught_as_nondeterministic() {
    let source = module(DELIVERY);
    let journal = json!([{ "seq": 0, "kind": "exec", "ok": null }]);
    let result = activate(&source, &json!({ "target": { "workflow": "entrega" }, "journal": journal, "item": {} }));
    assert_eq!(result["state"], "nondeterministic", "{result}");
}

#[test]
fn a_cancel_from_the_tracker_ends_the_workflow_cancelled() {
    let source = module(DELIVERY);
    let (result, _) = drive(&source, json!({ "workflow": "entrega" }), |command| match command["kind"].as_str() {
        Some("event") => Some(json!({ "kind": "tracker.status.changed" })),
        Some("sleep") => None,
        _ => Some(answer(command)),
    });
    assert_eq!(result["state"], "cancelled", "{result}");
    assert_eq!(result["reason"], "cancelado no tracker");
}

#[test]
fn a_step_list_goes_back_on_failure_and_sets_the_status_first() {
    let source = module(STEPS);
    let mut tests = 0;
    let mut asked = Vec::new();
    let (result, _) = drive(&source, json!({ "workflow": "lista" }), |command| {
        asked.push(command.clone());
        if command["kind"] == "exec" {
            tests += 1;
            return Some(json!({ "code": if tests == 1 { 1 } else { 0 }, "output": "falhou" }));
        }
        Some(Value::Null)
    });
    assert_eq!(result["state"], "completed", "{result}");
    let kinds: Vec<&str> = asked.iter().filter_map(|command| command["kind"].as_str()).collect();
    assert_eq!(kinds, ["status.set", "agent", "exec", "status.set", "agent", "exec"]);
    let attempts: Vec<(&str, i64)> = result["signals"]
        .as_array()
        .expect("signals")
        .iter()
        .filter(|signal| signal["kind"] == "step.started")
        .filter_map(|signal| Some((signal["step"].as_str()?, signal["attempt"].as_i64()?)))
        .collect();
    assert_eq!(attempts, [("develop", 1), ("checks", 1), ("develop", 2), ("checks", 2)]);
}

#[test]
fn a_view_is_plain_blocks_rendered_from_the_step_state() {
    let host: Arc<dyn script::Host> = Arc::new(NoHost);
    let stop = Arc::new(AtomicBool::new(false));
    let request = json!({ "step": "checks", "state": { "output": "erro na linha 3", "failed": true } });
    let blocks =
        script::run("sdk.js", &module(DELIVERY), "host.view", &[request], &Value::Null, host, stop).expect("view");
    assert_eq!(
        blocks,
        json!([
            { "type": "logs", "text": "erro na linha 3" },
            { "type": "actions", "buttons": [{ "id": "rerun", "label": "rerun" }] }
        ])
    );
}
