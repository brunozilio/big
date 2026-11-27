use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

struct NoHost;

impl script::Host for NoHost {
    fn call(&self, function: &str, _arguments: Value) -> Result<Value, String> {
        Err(format!("the sdk calls nothing on the host, yet it called {function}"))
    }
}

const STEPS: &str = r#"
const run = step({
  name: "run",
  label: "Run",
  async run(ctx, opts) {
    const command = ctx.each ? `${opts.command} ${ctx.each.value}` : opts.command;
    const r = await ctx.exec(command);
    const seen = Object.fromEntries(Object.entries(ctx.needs).map(([id, need]) => [id, need.outcome]));
    const data = { command, needs: seen, from: ctx.from ? ctx.from.step : null };
    return r.code === 0 ? ctx.pass(data) : ctx.fail({ ...data, error: r.output });
  },
});
"#;

struct Drive {
    result: Value,
    rounds: Vec<Vec<String>>,
    cancelled: Vec<String>,
    signals: Vec<Value>,
}

fn module(workflow: &str) -> String {
    let sdk = super::builtin_text("sdk/index.js").expect("the runtime is built in");
    format!("{sdk}\n{STEPS}\n{workflow}")
}

fn activate(source: &str, activation: &Value) -> Value {
    let host: Arc<dyn script::Host> = Arc::new(NoHost);
    let stop = Arc::new(AtomicBool::new(false));
    script::run("sdk.js", source, "host.activate", std::slice::from_ref(activation), &Value::Null, host, stop)
        .expect("activation")
}

fn label(command: &Value) -> String {
    match command["kind"].as_str().unwrap_or_default() {
        "exec" => {
            format!("exec {} @{}", command["args"]["command"].as_str().unwrap_or_default(), text(&command["step"]))
        }
        other => format!("{other} @{}", text(&command["step"])),
    }
}

fn text(value: &Value) -> String {
    value.as_str().map_or_else(|| "-".to_owned(), str::to_owned)
}

fn drive(workflow: &str, name: &str, mut reply: impl FnMut(&Value) -> Option<Value>) -> Drive {
    let source = module(workflow);
    let mut journal: Vec<Value> = Vec::new();
    let mut rounds = Vec::new();
    let mut cancelled = Vec::new();
    let mut issued: BTreeMap<u64, String> = BTreeMap::new();
    for round in 0..60 {
        let activation = json!({ "target": { "workflow": name }, "item": { "id": "i1", "title": "x" },
            "journal": journal, "start": 1000, "seed": "i1" });
        let result = activate(&source, &activation);
        assert_eq!(result, activate(&source, &activation), "the same journal replays to the same result");
        let pending = result["pending"].as_array().cloned().unwrap_or_default();
        for command in &pending {
            issued.insert(command["seq"].as_u64().unwrap_or_default(), label(command));
        }
        for seq in result["cancel"].as_array().cloned().unwrap_or_default() {
            let known = issued.get(&seq.as_u64().unwrap_or_default()).cloned().unwrap_or_default();
            if !cancelled.contains(&known) {
                cancelled.push(known);
            }
        }
        if result["state"] != "running" {
            let signals = result["signals"].as_array().cloned().unwrap_or_default();
            return Drive { result, rounds, cancelled, signals };
        }
        rounds.push(pending.iter().map(label).collect());
        let answered: Vec<Value> = pending
            .iter()
            .filter_map(|command| {
                let value = reply(command)?;
                Some(json!({ "seq": command["seq"], "kind": command["kind"], "ok": value, "at": 2000 + round }))
            })
            .collect();
        assert!(!answered.is_empty(), "nothing was answered in round {round}: {pending:?}");
        journal.extend(answered);
    }
    panic!("the workflow never ended")
}

fn exec(code: i64) -> Value {
    json!({ "code": code, "output": if code == 0 { "ok" } else { "broke" } })
}

fn retry() -> Value {
    json!({ "action": "retry" })
}

fn finished<'a>(drive: &'a Drive, step: &str) -> Vec<&'a Value> {
    drive.signals.iter().filter(|signal| signal["kind"] == "step.finished" && signal["step"] == step).collect()
}

#[test]
fn steps_without_needs_run_in_order_and_needs_open_them_side_by_side() {
    let flow = r#"
workflow({ name: "w", steps: [
  run({ id: "build", command: "build" }),
  run({ id: "lint", command: "lint" }),
  run({ id: "test", command: "test", needs: "build" }),
  run({ id: "ship", command: "ship", needs: ["lint", "test"] }),
] });
"#;
    let drive = drive(flow, "w", |_| Some(exec(0)));
    assert_eq!(drive.result["state"], "completed");
    assert_eq!(
        drive.rounds,
        [
            vec!["exec build @build".to_owned()],
            vec!["exec lint @lint".to_owned(), "exec test @test".to_owned()],
            vec!["exec ship @ship".to_owned()],
        ]
    );
    assert_eq!(finished(&drive, "ship")[0]["data"]["needs"], json!({ "lint": "passed", "test": "passed" }));
}

#[test]
fn a_group_runs_its_own_steps_under_its_path_and_hands_their_outputs_up() {
    let flow = r#"
workflow({ name: "w", steps: [
  run({ id: "develop", command: "develop" }),
  group({ id: "checks", label: "Checks", steps: [
    run({ id: "lint", command: "lint", needs: [] }),
    run({ id: "test", command: "test", needs: [] }),
    group({ id: "deep", needs: ["lint", "test"], steps: [run({ id: "e2e", command: "e2e" })] }),
  ] }),
  run({ id: "pr", command: "pr" }),
] });
"#;
    let drive = drive(flow, "w", |_| Some(exec(0)));
    assert_eq!(drive.result["state"], "completed");
    assert_eq!(drive.rounds[1], ["exec lint @checks/lint", "exec test @checks/test"]);
    assert_eq!(drive.rounds[2], ["exec e2e @checks/deep/e2e"]);
    let group = finished(&drive, "checks");
    assert_eq!(group[0]["outcome"], "passed");
    assert_eq!(group[0]["data"]["deep"]["e2e"]["command"], "e2e");
    let started = drive.signals.iter().find(|signal| signal["kind"] == "step.started" && signal["step"] == "checks");
    assert_eq!(started.map(|signal| signal["group"].clone()), Some(json!(true)));
}

#[test]
fn a_failure_inside_a_group_stops_its_siblings_and_asks_a_person_at_the_top() {
    let flow = r#"
workflow({ name: "w", steps: [
  group({ id: "checks", steps: [
    run({ id: "lint", command: "lint", needs: [] }),
    run({ id: "test", command: "test", needs: [] }),
    run({ id: "after", command: "after", needs: ["lint", "test"] }),
  ] }),
] });
"#;
    let mut asked = 0;
    let drive = drive(flow, "w", |command| match label(command).as_str() {
        "exec lint @checks/lint" => None,
        "exec test @checks/test" => Some(exec(1)),
        "wait.human @-" => {
            asked += 1;
            Some(json!({ "action": "cancel" }))
        }
        _ => Some(Value::Null),
    });
    assert_eq!(asked, 1);
    assert_eq!(drive.result["state"], "cancelled");
    assert_eq!(drive.cancelled, ["exec lint @checks/lint"]);
    assert!(finished(&drive, "checks/after").iter().all(|signal| signal["outcome"] == "skipped"));
    let resets =
        drive.signals.iter().filter(|signal| signal["kind"] == "step.reset" && signal["step"] == "checks/lint");
    assert_eq!(resets.count(), 1);
}

#[test]
fn a_step_deep_in_a_group_goes_back_to_a_step_at_the_top_and_the_rest_runs_again() {
    let flow = r#"
workflow({ name: "w", steps: [
  run({ id: "develop", command: "develop" }),
  group({ id: "checks", steps: [
    run({ id: "lint", command: "lint", needs: [] }),
    run({ id: "test", command: "test", needs: [], onFail: { goto: "develop", retries: 2 } }),
  ] }),
  run({ id: "pr", command: "pr" }),
] });
"#;
    let mut tests = 0;
    let drive = drive(flow, "w", |command| {
        let named = label(command);
        if named == "exec test @checks/test" {
            tests += 1;
            return Some(exec(i64::from(tests < 3)));
        }
        Some(exec(0))
    });
    assert_eq!(drive.result["state"], "completed");
    let develops = finished(&drive, "develop");
    assert_eq!(develops.len(), 3);
    assert_eq!(develops[1]["data"]["from"], "checks/test");
    assert_eq!(finished(&drive, "pr").len(), 1);
}

#[test]
fn going_back_into_a_group_reruns_only_the_target_and_what_needs_it() {
    let flow = r#"
workflow({ name: "w", steps: [
  group({ id: "checks", steps: [
    run({ id: "lint", command: "lint", needs: [] }),
    run({ id: "test", command: "test", needs: [] }),
    run({ id: "report", command: "report", needs: "test" }),
  ] }),
  run({ id: "review", command: "review", onFail: "checks/test", retries: 1 }),
] });
"#;
    let mut reviews = 0;
    let drive = drive(flow, "w", |command| {
        if label(command) == "exec review @review" {
            reviews += 1;
            return Some(exec(i64::from(reviews == 1)));
        }
        Some(exec(0))
    });
    assert_eq!(drive.result["state"], "completed");
    assert_eq!(finished(&drive, "checks/lint").len(), 1);
    assert_eq!(finished(&drive, "checks/test").len(), 2);
    assert_eq!(finished(&drive, "checks/report").len(), 2);
    assert_eq!(finished(&drive, "checks/test")[1]["data"]["from"], "review");
}

#[test]
fn continue_lets_the_next_steps_run_and_failure_steps_run_only_after_a_failure() {
    let flow = r#"
workflow({ name: "w", steps: [
  group({ id: "checks", steps: [
    run({ id: "flaky", command: "flaky", onFail: "continue" }),
    run({ id: "after", command: "after" }),
    run({ id: "cleanup", command: "cleanup", needs: [], when: "failure" }),
    run({ id: "always", command: "always", needs: ["after"], when: "always" }),
  ] }),
] });
"#;
    let drive = drive(flow, "w", |command| Some(exec(i64::from(label(command).starts_with("exec flaky")))));
    assert_eq!(drive.result["state"], "completed");
    assert_eq!(finished(&drive, "checks/after")[0]["outcome"], "passed");
    assert_eq!(finished(&drive, "checks/always")[0]["outcome"], "passed");
    assert_eq!(finished(&drive, "checks/cleanup")[0]["outcome"], "skipped");
    assert_eq!(finished(&drive, "checks")[0]["outcome"], "passed");
}

#[test]
fn a_failure_step_cleans_up_when_a_sibling_fails_and_the_group_still_fails() {
    let flow = r#"
workflow({ name: "w", steps: [
  group({ id: "deploy", onFail: "fail", steps: [
    run({ id: "push", command: "push" }),
    run({ id: "rollback", command: "rollback", when: "failure" }),
  ] }),
] });
"#;
    let drive = drive(flow, "w", |command| Some(exec(i64::from(label(command).starts_with("exec push")))));
    assert_eq!(drive.result["state"], "failed");
    assert_eq!(finished(&drive, "deploy/rollback")[0]["outcome"], "passed");
    assert_eq!(finished(&drive, "deploy")[0]["data"]["failed"], "deploy/push");
}

#[test]
fn a_step_retries_itself_before_its_failure_policy() {
    let flow = r#"
workflow({ name: "w", steps: [run({ id: "net", command: "net", rerun: 2 })] });
"#;
    let mut calls = 0;
    let drive = drive(flow, "w", |_| {
        calls += 1;
        Some(exec(i64::from(calls < 3)))
    });
    assert_eq!(drive.result["state"], "completed");
    assert_eq!(finished(&drive, "net").len(), 3);
    assert_eq!(finished(&drive, "net")[2]["data"]["from"], "net");
}

#[test]
fn for_each_runs_one_lane_per_value_up_to_the_concurrency() {
    let flow = r#"
workflow({ name: "w", steps: [
  run({ id: "repo", command: "check", forEach: () => ["a", "b", "c"], concurrency: 2 }),
  run({ id: "after", command: "after" }),
] });
"#;
    let drive = drive(flow, "w", |_| Some(exec(0)));
    assert_eq!(drive.result["state"], "completed");
    assert_eq!(drive.rounds[0], ["exec check a @repo", "exec check b @repo"]);
    assert_eq!(drive.rounds[1], ["exec check c @repo"]);
    let lanes: Vec<Value> = finished(&drive, "repo").iter().map(|signal| signal["branch"].clone()).collect();
    assert_eq!(lanes, [json!("main/repo.0"), json!("main/repo.1"), json!("main/repo.2")]);
}

#[test]
fn a_failed_lane_stops_the_others_and_a_person_can_try_again() {
    let flow = r#"
workflow({ name: "w", steps: [run({ id: "repo", command: "check", forEach: ["a", "b"] })] });
"#;
    let mut asked = false;
    let mut failed = false;
    let drive = drive(flow, "w", |command| match label(command).as_str() {
        "exec check a @repo" if !failed => {
            failed = true;
            Some(exec(1))
        }
        "exec check b @repo" if !asked => None,
        "wait.human @-" => {
            asked = true;
            Some(retry())
        }
        _ => Some(exec(0)),
    });
    assert_eq!(drive.result["state"], "completed");
    assert!(drive.cancelled.contains(&"exec check b @repo".to_owned()));
}

#[test]
fn a_workflow_is_reused_inside_another_as_a_group() {
    let flow = r#"
const quality = workflow({ name: "quality", steps: [run({ id: "lint", command: "lint" }), run({ id: "test", command: "test" })] });
workflow({ name: "w", steps: [run({ id: "develop", command: "develop" }), use(quality), use("quality", { id: "again" })] });
"#;
    let drive = drive(flow, "w", |_| Some(exec(0)));
    assert_eq!(drive.result["state"], "completed");
    let commands: Vec<String> = drive.rounds.iter().flatten().cloned().collect();
    assert_eq!(
        commands,
        [
            "exec develop @develop",
            "exec lint @quality/lint",
            "exec test @quality/test",
            "exec lint @again/lint",
            "exec test @again/test",
        ]
    );
}

#[test]
fn a_composite_step_and_a_flow_inside_code_are_groups_too() {
    let flow = r#"
const verify = step({ name: "verify", label: "Verify", steps: (opts) => opts.commands.map((command) => run({ id: command, command, needs: [] })) });
const custom = step({ name: "custom", async run(ctx) {
  const result = await ctx.flow([run({ id: "one", command: "one" }), run({ id: "two", command: "two" })]);
  return ctx.pass(result.data);
} });
workflow({ name: "w", steps: [verify({ commands: ["lint", "test"] }), custom()] });
"#;
    let drive = drive(flow, "w", |_| Some(exec(0)));
    assert_eq!(drive.result["state"], "completed");
    assert_eq!(drive.rounds[0], ["exec lint @verify/lint", "exec test @verify/test"]);
    assert_eq!(drive.rounds[1], ["exec one @custom/one"]);
    assert_eq!(finished(&drive, "custom")[0]["data"]["two"]["command"], "two");
}

#[test]
fn a_condition_skips_a_step_and_a_function_decides_the_failure_policy() {
    let flow = r#"
workflow({ name: "w", steps: [
  run({ id: "plan", command: "plan" }),
  run({ id: "infra", command: "infra", if: (ctx) => ctx.needs.plan.data.command === "nothing" }),
  run({ id: "test", command: "test", onFail: (ctx, result) => (result.data.error === "broke" ? "continue" : "ask") }),
  run({ id: "pr", command: "pr" }),
] });
"#;
    let drive = drive(flow, "w", |command| Some(exec(i64::from(label(command).starts_with("exec test")))));
    assert_eq!(drive.result["state"], "completed");
    assert_eq!(finished(&drive, "infra")[0]["outcome"], "skipped");
    assert_eq!(finished(&drive, "pr")[0]["outcome"], "passed");
}

#[test]
fn steps_that_need_each_other_in_a_cycle_are_refused() {
    let flow = r#"
workflow({ name: "w", steps: [run({ id: "a", command: "a", needs: "b" }), run({ id: "b", command: "b", needs: "a" })] });
"#;
    let source = module(flow);
    let activation =
        json!({ "target": { "workflow": "w" }, "item": { "id": "i1" }, "journal": [], "start": 0, "seed": "i1" });
    let host: Arc<dyn script::Host> = Arc::new(NoHost);
    let stop = Arc::new(AtomicBool::new(false));
    let found = script::run("sdk.js", &source, "host.activate", &[activation], &Value::Null, host, stop);
    let shown = format!("{found:?}");
    assert!(shown.contains("cycle"), "{shown}");
}

#[test]
fn the_description_is_a_tree_with_paths_needs_and_group_labels() {
    let flow = r#"
workflow({ name: "w", statuses: ["A"], steps: [
  run({ id: "develop", command: "develop", status: "Doing" }),
  group({ id: "checks", label: "Checks", steps: [run({ id: "lint", command: "lint", needs: [] }), run({ id: "test", command: "test", doneStatus: "Tested" })] }),
] });
"#;
    let source = module(flow);
    let host: Arc<dyn script::Host> = Arc::new(NoHost);
    let stop = Arc::new(AtomicBool::new(false));
    let described = script::run("sdk.js", &source, "host.describe", &[], &Value::Null, host, stop).expect("describe");
    let shape = &described["workflows"][0];
    assert_eq!(shape["steps"][1]["path"], "checks");
    assert_eq!(shape["steps"][1]["needs"], json!(["develop"]));
    assert_eq!(shape["steps"][1]["steps"][0]["path"], "checks/lint");
    assert_eq!(shape["steps"][1]["steps"][0]["needs"], json!([]));
    assert_eq!(shape["steps"][1]["steps"][1]["needs"], json!(["lint"]));
    assert_eq!(described["nodes"]["checks"]["label"], "Checks");
    assert_eq!(described["labels"]["run"]["label"], "Run");
}

const COMPOSED: &str = r#"
import { workflow, group } from "sdk";
import { command, agent } from "sdk/steps";

export default workflow({
  name: "montado",
  steps: [
    group({ id: "quality", label: "Quality", steps: [
      command({ id: "lint", run: "make lint", needs: [] }),
      command({ id: "test", run: ["make test", "make e2e"], needs: [] }),
      agent({ id: "verifier", agent: "verifier", prompt: (ctx) => `check ${ctx.needs.test.data.ran.length} commands`, reject: "REJECTED", needs: ["lint", "test"] }),
    ] }),
  ],
});
"#;

#[test]
fn a_workflow_made_only_of_groups_commands_and_agents_runs_through_the_bundler() {
    let source = super::bundle::bundle("workflows/montado.ts", COMPOSED).expect("bundle");
    let mut journal: Vec<Value> = Vec::new();
    let mut prompts: Vec<String> = Vec::new();
    for round in 0..20 {
        let activation =
            json!({ "item": { "id": "i1", "title": "x" }, "journal": journal, "start": 1000, "seed": "i1" });
        let result = activate(&source, &activation);
        if result["state"] != "running" {
            assert_eq!(result["state"], "completed", "{result}");
            assert_eq!(prompts.len(), 1);
            assert!(prompts[0].starts_with("check 2 commands"), "{prompts:?}");
            return;
        }
        for command in result["pending"].as_array().cloned().unwrap_or_default() {
            let value = match command["kind"].as_str() {
                Some("exec") => exec(0),
                Some("agent") => {
                    prompts.push(command["args"]["input"]["prompt"].as_str().unwrap_or_default().to_owned());
                    json!({ "text": "VERDICT: APPROVED", "session": "s1" })
                }
                _ => Value::Null,
            };
            journal.push(json!({ "seq": command["seq"], "kind": command["kind"], "ok": value, "at": 2000 + round }));
        }
    }
    panic!("the workflow never ended")
}
