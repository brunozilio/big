use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use super::bundle::bundle;

struct NoHost;

impl script::Host for NoHost {
    fn call(&self, function: &str, _arguments: Value) -> Result<Value, String> {
        Err(format!("the sdk calls nothing on the host, yet it called {function}"))
    }
}

const STEPS: [&str; 10] = ["plan", "ticket", "cases", "develop", "checks", "pr", "ci", "review", "merge", "release"];

const PROVIDERS: [&str; 9] =
    ["jira", "github", "aws", "atlassian", "gitlab", "bitbucket", "amazon", "zephyr", "smartbear"];

const CUSTOM: &str = r#"
import { workflow, step, markdown, actions } from "sdk";
import { plan, develop, checks } from "sdk/steps";

const qa = step({
  name: "qa",
  view: (s) => [markdown(s.notes ?? "")],
  async run(ctx) {
    const answer = await ctx.waitHuman([markdown("Teste manual"), actions(["ok", "falhou"])]);
    return answer.action === "ok" ? ctx.pass({ notes: "ok" }) : ctx.fail({ notes: "falhou" });
  },
});

export default workflow({
  name: "rapido",
  statuses: ["Backlog", "Em curso", "Pronto"],
  async run(ctx, item) {
    await ctx.status("Backlog");
    if (!item.labels.includes("hotfix")) await plan(ctx, { agent: "planner" });
    await ctx.status("Em curso");
    await develop(ctx, { agent: "developer", branch: "feature/x" });
    const [checked, scan] = await ctx.all([
      (branch) => checks(branch, { commands: ["make test"] }),
      (branch) => branch.child("varredura", { ref: item.branch }),
    ]);
    const first = await ctx.race([ctx.event("tracker.status.changed", { to: "Cancelado" }), (branch) => qa(branch)]);
    if (first.kind === "event") return ctx.cancel("cancelado no tracker");
    await ctx.status("Pronto");
    return { scan, checks: checked.data.output, qa: first.value.data.notes };
  },
});
"#;

fn source(path: &str, text: &str) -> String {
    bundle(path, text).unwrap_or_else(|error| panic!("bundle: {error}"))
}

fn default_workflow() -> String {
    let text = super::builtin_text("workflows/default.ts").expect("the default workflow is built in");
    source("workflows/default.ts", &text)
}

fn call(source: &str, function: &str, arguments: &[Value]) -> Value {
    let host: Arc<dyn script::Host> = Arc::new(NoHost);
    let stop = Arc::new(AtomicBool::new(false));
    script::run("workflow.js", source, function, arguments, &Value::Null, host, stop)
        .unwrap_or_else(|error| panic!("{function}: {error}"))
}

fn drive(source: &str, item: &Value, mut reply: impl FnMut(&Value) -> Option<Value>) -> (Value, Vec<Value>) {
    let mut journal: Vec<Value> = Vec::new();
    let mut asked: Vec<Value> = Vec::new();
    for round in 0..200 {
        let activation = json!({ "item": item, "journal": journal, "start": 1000, "seed": item["id"] });
        let result = call(source, "host.activate", std::slice::from_ref(&activation));
        if result["state"] != "running" {
            assert_eq!(result, call(source, "host.activate", &[activation]), "the same journal replays the same");
            return (result, asked);
        }
        let pending = result["pending"].as_array().expect("pending").clone();
        let mut answered = false;
        for command in &pending {
            if !asked.iter().any(|known| known["seq"] == command["seq"]) {
                asked.push(command.clone());
            }
            let Some(value) = reply(command) else { continue };
            answered = true;
            let entry = match value.get("failed") {
                Some(error) => {
                    json!({ "seq": command["seq"], "kind": command["kind"], "error": error, "at": 2000 + round })
                }
                None => json!({ "seq": command["seq"], "kind": command["kind"], "ok": value, "at": 2000 + round }),
            };
            journal.push(entry);
        }
        assert!(answered, "the fake host answers something each round: {result}");
    }
    panic!("the workflow never ended")
}

#[derive(Default)]
struct Fake {
    reject_deploy: bool,
    lint_runs: usize,
    ci_runs: usize,
    reviews: usize,
    release_polls: usize,
    deployments_listed: usize,
}

impl Fake {
    fn reply(&mut self, command: &Value) -> Option<Value> {
        let args = &command["args"];
        match command["kind"].as_str()? {
            "agent" => Some(agent(args)),
            "exec" => Some(self.exec(args)),
            "call" => Some(self.call(args)),
            "wait.poll" => Some(self.poll(args)),
            "wait.human" => Some(self.human(args)),
            "event" => None,
            _ => Some(Value::Null),
        }
    }

    fn exec(&mut self, args: &Value) -> Value {
        let lint = args["command"] == "lint";
        self.lint_runs += usize::from(lint);
        let failing = lint && self.lint_runs == 1;
        let output = if failing { "lint: erro em src/a.rs" } else { "ok" };
        json!({ "code": i32::from(failing), "output": output, "stdout": output, "stderr": "" })
    }

    fn call(&mut self, args: &Value) -> Value {
        let inner = &args["args"];
        match (args["capability"].as_str(), args["action"].as_str()) {
            (Some("tracker"), Some("create_issue")) => {
                json!({ "ref": "T-9", "title": inner["title"], "status": "A fazer", "url": "https://tracker/T-9" })
            }
            (Some("code"), Some("open_pull_request")) => json!({
                "ref": "PR-7", "number": 7, "title": inner["title"], "body": inner["body"], "url": "https://code/7",
                "head": inner["head"], "base": "main", "state": "open", "draft": false, "mergeable": true,
                "checks": [], "updated": ""
            }),
            (Some("pipelines"), Some("runs")) if inner["ref"] == "abc" => {
                json!([{ "ref": "REL-1", "state": "running" }])
            }
            (Some("pipelines"), Some("runs")) => {
                self.ci_runs += 1;
                let runs: Vec<Value> =
                    (1..=self.ci_runs).rev().map(|n| json!({ "ref": format!("CI-{n}"), "state": "running" })).collect();
                Value::Array(runs)
            }
            (Some("pipelines"), Some("pending_deployments")) => {
                self.deployments_listed += 1;
                if self.deployments_listed > 1 {
                    return json!([]);
                }
                json!([{ "id": "D1", "environment": "producao", "run": "REL-1", "reviewers": [], "canApprove": true }])
            }
            (Some("code"), Some("review_comments")) => json!([
                { "id": "c1", "thread": "t1", "path": "src/a.rs", "line": 3, "author": "ana", "body": "trate o erro", "resolved": false, "at": "" },
                { "id": "c2", "thread": "t2", "path": "src/b.rs", "line": 1, "author": "ana", "body": "resolvido", "resolved": true, "at": "" }
            ]),
            (Some("code"), Some("merge")) => json!({ "merged": true, "sha": "abc" }),
            (Some("tracker"), Some("issue")) => {
                json!({ "ref": inner["issue"], "id": "10009", "title": "x", "status": "A fazer", "url": "" })
            }
            (Some("tests"), Some("create_case")) => {
                json!({ "ref": "Z-T1", "title": inner["title"], "url": "https://tests/Z-T1" })
            }
            (Some("tests"), Some("create_cycle")) => json!({ "ref": "Z-R1", "url": "https://tests/Z-R1" }),
            _ => json!({ "ok": true }),
        }
    }

    fn poll(&mut self, args: &Value) -> Value {
        let run = args["call"]["args"]["run"].as_str().unwrap_or_default().to_owned();
        if run.starts_with("CI-") {
            return json!({ "ref": run, "state": "passed", "jobs": [] });
        }
        self.release_polls += 1;
        let state = if self.release_polls == 1 { "waiting" } else { "passed" };
        json!({ "ref": run, "state": state, "jobs": [] })
    }

    fn human(&mut self, args: &Value) -> Value {
        let view = args["view"].as_array().cloned().unwrap_or_default();
        let has = |kind: &str| view.iter().any(|block| block["type"] == kind);
        let buttons: Vec<Value> =
            view.iter().flat_map(|block| block["buttons"].as_array().cloned().unwrap_or_default()).collect();
        let offers = |id: &str| buttons.iter().any(|button| button["id"] == id);
        if has("review") {
            self.reviews += 1;
            if self.reviews == 1 {
                return json!({ "action": "request_changes", "values": { "body": "faltou teste" }, "by": "ana" });
            }
            return json!({ "action": "approve", "values": {}, "by": "ana" });
        }
        if has("merge") {
            return json!({ "action": "merge", "values": { "method": "squash" }, "by": "ana" });
        }
        if view.iter().any(|block| block["approve"] == "approve_cases") {
            return json!({ "action": "approve_cases", "values": {}, "by": "ana" });
        }
        if offers("approve_deploy") {
            let action = if self.reject_deploy { "reject_deploy" } else { "approve_deploy" };
            return json!({ "action": action, "values": {}, "by": "ana" });
        }
        json!({ "action": "cancel", "values": {}, "by": "ana" })
    }
}

fn agent(args: &Value) -> Value {
    match args["name"].as_str() {
        Some("planner") => {
            json!({ "text": "plano", "session": "plan-1", "output": { "plan": "1. mudar x", "title": "Corrigir x" } })
        }
        Some("developer") => json!({ "text": "feito", "session": "dev-1", "output": { "branch": "item/i1" } }),
        Some("release") => json!({ "text": "", "session": "rel-1", "output": { "notes": "## Notas\n- x corrigido" } }),
        Some("verifier") => json!({ "text": "✓ x\nVEREDITO: APROVADO", "session": "ver-1", "output": null }),
        Some("tester") => tester(args),
        _ => json!({ "text": "", "session": "other", "output": null }),
    }
}

fn tester(args: &Value) -> Value {
    let prompt = args["input"]["prompt"].as_str().unwrap_or_default();
    if prompt.starts_with("Automatize") {
        let results = json!([{ "case": "Z-T1", "status": "passed", "evidence": "api.spec.ts passou" }]);
        return json!({ "text": "", "session": "test-2", "output": { "results": results } });
    }
    let cases = json!([{ "title": "Cadastrar cliente pela API", "kind": "api", "objective": "salva",
        "given": ["nenhum cliente Ana"], "when": ["envio POST /clientes com nome=Ana"], "then": ["a resposta tem status 201", "Ana fica gravada"] }]);
    json!({ "text": "", "session": "test-1", "output": { "cases": cases } })
}

fn item(issue: Value) -> Value {
    json!({ "id": "i1", "workflow": "entrega", "title": "x", "labels": [], "branch": null, "issue": issue,
        "input": null, "checks": ["lint", "test"] })
}

fn calls<'a>(asked: &'a [Value], action: &str) -> Vec<&'a Value> {
    asked.iter().filter(|command| command["kind"] == "call" && command["args"]["action"] == action).collect()
}

fn of_kind<'a>(asked: &'a [Value], kind: &str) -> Vec<&'a Value> {
    asked.iter().filter(|command| command["kind"] == kind).collect()
}

#[test]
fn every_step_and_the_default_workflow_are_built_in_typed_and_name_no_provider() {
    let mut paths: Vec<String> =
        STEPS.iter().flat_map(|step| [format!("sdk/steps/{step}.js"), format!("sdk/steps/{step}.d.ts")]).collect();
    paths.extend(
        [
            "sdk/steps/index.js",
            "sdk/steps/index.d.ts",
            "sdk/steps/types.d.ts",
            "sdk/steps/context.js",
            "sdk/steps/context.d.ts",
            "sdk/steps/e2e.js",
            "workflows/default.ts",
            "workflows/pull-request.ts",
        ]
        .map(str::to_owned),
    );
    for path in &paths {
        let text = super::builtin_text(path).unwrap_or_else(|| panic!("{path} is built in"));
        let lower = text.to_lowercase();
        for provider in PROVIDERS {
            assert!(!lower.contains(provider), "{path} names {provider}");
        }
        assert!(!text.contains("//") || text.contains("://"), "{path} has no comments");
    }
    assert!(
        super::builtin_files("sdk/steps/").len() >= paths.iter().filter(|path| path.starts_with("sdk/steps/")).count()
    );
    let index = super::builtin_text("sdk/steps/index.js").expect("index");
    assert!(STEPS.iter().all(|step| index.contains(&format!("default as {step} "))));
}

#[test]
fn the_default_workflow_describes_its_statuses_and_the_ten_steps_in_order() {
    let described = call(&default_workflow(), "host.describe", &[]);
    let workflow = &described["workflows"][0];
    assert_eq!(workflow["name"], "entrega");
    assert_eq!(workflow["statuses"], json!(["A fazer", "Fazendo", "Revisando", "Feito"]));
    let ids: Vec<&str> =
        workflow["steps"].as_array().expect("steps").iter().filter_map(|call| call["id"].as_str()).collect();
    assert_eq!(ids, STEPS);
    assert_eq!(workflow["steps"][2]["options"]["agent"], "tester");
    assert_eq!(workflow["steps"][4]["options"]["onFail"], "develop");
    assert_eq!(workflow["steps"][4]["options"]["agent"], "verifier");
    assert_eq!(workflow["steps"][4]["options"]["tester"], "tester");
    assert_eq!(workflow["steps"][6]["options"]["onFail"], "develop");
    assert_eq!(workflow["steps"][7]["options"]["onChanges"], "develop");
    assert_eq!(described["steps"], json!(STEPS));
}

#[test]
fn the_ticket_goes_to_the_project_and_connection_chosen_for_the_item() {
    let mut fake = Fake::default();
    let mut chosen = item(Value::Null);
    chosen["project"] = json!("OPS");
    chosen["connection"] = json!("c9");
    let (_, asked) = drive(&default_workflow(), &chosen, |command| fake.reply(command));
    let created = calls(&asked, "create_issue");
    assert_eq!(created[0]["args"]["args"]["project"], "OPS");
    assert_eq!(created[0]["connection"], "c9");
}

fn shows_the_reviewer_and_asks_for_portuguese(asked: &[Value]) {
    let has_review = |command: &&Value| {
        command["args"]["view"].as_array().is_some_and(|view| view.iter().any(|block| block["type"] == "review"))
    };
    let reviewing = of_kind(asked, "wait.human").into_iter().find(has_review).expect("a review waits for a person");
    let view = reviewing["args"]["view"].as_array().expect("view");
    assert_eq!(
        view.iter().find(|block| block["type"] == "review").map(|block| &block["session"]),
        Some(&json!("other"))
    );
    assert!(view.iter().all(|block| block["type"] != "actions"), "the review block has its own buttons: {view:?}");
    let agents = of_kind(asked, "agent");
    let prompts: Vec<&str> = agents.iter().filter_map(|command| command["args"]["input"]["prompt"].as_str()).collect();
    assert!(
        !prompts.is_empty() && prompts.iter().all(|prompt| prompt.ends_with("português do Brasil.")),
        "{prompts:?}"
    );
}

#[test]
fn the_default_workflow_delivers_an_item_end_to_end() {
    let mut fake = Fake::default();
    let (result, asked) = drive(&default_workflow(), &item(Value::Null), |command| fake.reply(command));
    assert_eq!(result["state"], "completed", "{result}");
    assert_eq!(result["result"]["outcome"], "passed");
    let statuses: Vec<&Value> = of_kind(&asked, "status.set").iter().map(|command| &command["args"]["name"]).collect();
    assert_eq!(statuses, ["A fazer", "Fazendo", "Fazendo", "Revisando", "Fazendo", "Revisando", "Feito"]);
    let planner = &of_kind(&asked, "agent")[0];
    assert_eq!(planner["args"]["name"], "planner");
    assert_eq!(planner["args"]["mode"], "plan");
    shows_the_reviewer_and_asks_for_portuguese(&asked);
    let created = calls(&asked, "create_issue");
    assert_eq!(created.len(), 1);
    assert_eq!(
        created[0]["args"]["args"],
        json!({ "title": "Corrigir x", "description": "1. mudar x" }),
        "no format, the whole plan"
    );
    let developers: Vec<&Value> =
        of_kind(&asked, "agent").into_iter().filter(|command| command["args"]["name"] == "developer").collect();
    assert_eq!(developers.len(), 3, "develop, back from checks, back from review");
    assert!(
        developers[0]["args"]["input"]["prompt"]
            .as_str()
            .is_some_and(|prompt| prompt.contains("1. mudar x") && prompt.contains("T-9"))
    );
    assert!(
        developers[1]["args"]["input"]["prompt"]
            .as_str()
            .is_some_and(|prompt| prompt.contains("lint: erro em src/a.rs"))
    );
    let back = developers[2]["args"]["input"]["prompt"].as_str().expect("prompt");
    assert!(back.contains("src/a.rs:3: trate o erro") && back.contains("faltou teste") && !back.contains("resolvido"));
    assert!(developers[1..].iter().all(|command| command["args"]["input"]["session"] == "dev-1"));
    assert!(developers[1..].iter().all(|command| command["args"]["worktree"] == "item/i1"));
    let opened = calls(&asked, "open_pull_request");
    assert_eq!(opened.len(), 1, "the pull request opens once and is reused");
    assert_eq!(opened[0]["args"]["args"]["head"], "item/i1");
    let polled: Vec<&Value> =
        of_kind(&asked, "wait.poll").iter().map(|command| &command["args"]["call"]["args"]["run"]).collect();
    assert_eq!(
        polled,
        ["CI-1", "CI-2", "REL-1", "REL-1", "REL-1"],
        "a new run per CI attempt and the run raced with the approval"
    );
    assert!(of_kind(&asked, "event").iter().any(|command| command["args"]["kind"] == "pipelines.run.finished"));
    let verdicts: Vec<&Value> =
        calls(&asked, "submit_review").iter().map(|command| &command["args"]["args"]["verdict"]).collect();
    assert_eq!(verdicts, ["request_changes", "approve"]);
    assert_eq!(calls(&asked, "merge")[0]["args"]["args"], json!({ "pr": "PR-7", "method": "squash" }));
    let reviewed = calls(&asked, "review_deployment");
    assert_eq!(reviewed[0]["args"]["args"], json!({ "run": "REL-1", "deployments": ["D1"], "decision": "approve" }));
    assert!(of_kind(&asked, "impediment.set").is_empty());
    let named = |name: &str| -> Vec<&Value> {
        of_kind(&asked, "agent").into_iter().filter(|command| command["args"]["name"] == name).collect()
    };
    let reviewers = named("reviewer");
    assert_eq!(reviewers.len(), 2, "the reviewer reads the pull request before each human verdict");
    assert!(
        reviewers.iter().all(|command| command["args"]["mode"] == "plan" && command["args"]["input"]["pr"] == "PR-7")
    );
    let releases = named("release");
    assert_eq!(releases.len(), 1, "the release agent writes the notes once the deploy passed");
    assert_eq!(releases[0]["args"]["mode"], "plan");
    assert_eq!(releases[0]["args"]["cloud"], true);
    assert!(releases[0]["args"]["input"]["prompt"].as_str().is_some_and(|prompt| prompt.contains("REL-1")));
}

#[test]
fn ci_without_any_pipeline_moves_on_and_says_why() {
    let text = r#"
import { workflow } from "sdk";
import { ci } from "sdk/steps";
export default workflow({
  name: "so-ci",
  async run(ctx) {
    const done = await ci(ctx, { ref: "big/x", patience: 10 });
    return { outcome: done.outcome, state: done.data.state };
  },
});
"#;
    let (result, asked) =
        drive(&source("workflows/so-ci.ts", text), &item(json!("T-1")), |command| match command["kind"].as_str() {
            Some("call") => Some(json!([])),
            _ => Some(Value::Null),
        });
    let runs = calls(&asked, "runs");
    assert_eq!(runs.len(), 3, "twice for the branch, then once for the whole repository");
    assert_eq!(runs[2]["args"]["args"].get("ref"), None, "the repository never ran any pipeline");
    assert_eq!(result["result"], json!({ "outcome": "skipped", "state": "none" }), "{result}");
}

#[test]
fn ci_keeps_waiting_when_the_repository_has_pipelines() {
    let text = r#"
import { workflow } from "sdk";
import { ci } from "sdk/steps";
export default workflow({
  name: "so-ci",
  async run(ctx) {
    const done = await ci(ctx, { ref: "big/x", patience: 4 });
    return { outcome: done.outcome, state: done.data.state };
  },
});
"#;
    let (result, asked) =
        drive(&source("workflows/so-ci.ts", text), &item(json!("T-1")), |command| match command["kind"].as_str() {
            Some("call") if command["args"]["args"].get("ref").is_none() => {
                Some(json!([{ "ref": "old", "state": "passed" }]))
            }
            Some("call") => Some(json!([])),
            _ => Some(Value::Null),
        });
    assert_eq!(calls(&asked, "runs").len(), 5, "four for the branch and one look at the repository");
    assert_eq!(result["result"]["state"], "none", "{result}");
}

#[test]
fn a_failed_look_at_the_repository_keeps_the_ci_waiting_instead_of_failing() {
    let text = r#"
import { workflow } from "sdk";
import { ci } from "sdk/steps";
export default workflow({
  name: "so-ci",
  async run(ctx) {
    const done = await ci(ctx, { ref: "big/x", patience: 3 });
    return { outcome: done.outcome, state: done.data.state };
  },
});
"#;
    let (result, asked) =
        drive(&source("workflows/so-ci.ts", text), &item(json!("T-1")), |command| match command["kind"].as_str() {
            Some("call") if command["args"]["args"].get("ref").is_none() => {
                Some(json!({ "failed": { "kind": "permanent", "message": "ref is missing" } }))
            }
            Some("call") => Some(json!([])),
            _ => Some(Value::Null),
        });
    assert_eq!(calls(&asked, "runs").len(), 4, "three for the branch and the failed look");
    assert_eq!(result["result"], json!({ "outcome": "skipped", "state": "none" }), "{result}");
}

#[test]
fn a_release_without_a_deploy_pipeline_is_skipped_and_still_marks_the_item_done() {
    let text = r#"
import { workflow } from "sdk";
import { release } from "sdk/steps";
export default workflow({
  name: "so-release",
  statuses: ["Feito"],
  steps: [release({ ref: "abc", patience: 2, doneStatus: "Feito" })],
});
"#;
    let (result, asked) = drive(&source("workflows/so-release.ts", text), &item(json!("T-1")), |command| {
        match command["kind"].as_str() {
            Some("call") => Some(json!([])),
            _ => Some(Value::Null),
        }
    });
    assert_eq!(result["state"], "completed", "{result}");
    let statuses: Vec<&Value> = of_kind(&asked, "status.set").iter().map(|command| &command["args"]["name"]).collect();
    assert_eq!(statuses, [&json!("Feito")]);
    assert!(of_kind(&asked, "agent").is_empty(), "nothing was deployed, so there are no notes to write");
}

#[test]
fn a_reviewer_that_only_wrote_its_verdict_still_reaches_the_pull_request() {
    let mut fake = Fake::default();
    let written = "Vou revisar.Li os arquivos.\n## Veredito: `request_changes`\n\n## Comentários por linha\n1. src/a.ts:5 lê a env cedo";
    let (_, asked) = drive(&default_workflow(), &item(json!("T-1")), |command| {
        if command["kind"] == "agent" && command["args"]["name"] == "reviewer" {
            return Some(json!({ "text": written, "session": "rev-1", "output": null }));
        }
        fake.reply(command)
    });
    let posted = calls(&asked, "submit_review");
    let body = posted[0]["args"]["args"]["body"].as_str().expect("body");
    assert!(body.starts_with("## Veredito") && body.contains("src/a.ts:5"), "{body}");
    assert_eq!(posted[0]["args"]["args"]["verdict"], "comment");
}

#[test]
fn ci_ignores_the_runs_github_starts_by_itself() {
    let text = r#"
import { workflow } from "sdk";
import { ci } from "sdk/steps";
export default workflow({
  name: "so-ci",
  async run(ctx) {
    const done = await ci(ctx, { ref: "big/x", patience: 5 });
    return { outcome: done.outcome, state: done.data.state };
  },
});
"#;
    let dependabot = json!([{ "ref": "D1", "state": "queued", "event": "dynamic", "file": "dynamic/dependabot/dependabot-updates" }]);
    let (result, asked) =
        drive(&source("workflows/so-ci.ts", text), &item(json!("T-1")), |command| match command["kind"].as_str() {
            Some("call") => Some(dependabot.clone()),
            _ => Some(Value::Null),
        });
    assert!(of_kind(&asked, "wait.poll").is_empty(), "nobody waits for Dependabot");
    assert_eq!(result["result"], json!({ "outcome": "skipped", "state": "none" }), "{result}");
}

fn ci_with_a_dependabot_run(skip: Value) -> (Value, Vec<Value>) {
    let text = r#"
import { workflow } from "sdk";
import { ci } from "sdk/steps";
export default workflow({
  name: "so-ci",
  async run(ctx) {
    const done = await ci(ctx, { ref: "big/x", patience: 3 });
    return { outcome: done.outcome, state: done.data.state };
  },
});
"#;
    let mut chosen = item(json!("T-1"));
    chosen["application"] = json!({ "id": "app", "name": "Loja", "repositories": [], "skipDependabot": skip });
    let bot = json!([{ "ref": "B1", "state": "passed", "event": "pull_request", "actor": "dependabot[bot]" }]);
    drive(&source("workflows/so-ci.ts", text), &chosen, |command| match command["kind"].as_str() {
        Some("call") => Some(bot.clone()),
        Some("wait.poll") => Some(json!({ "ref": "B1", "state": "passed", "jobs": [] })),
        Some("event") => None,
        _ => Some(Value::Null),
    })
}

#[test]
fn the_application_s_flag_decides_whether_dependabot_runs_count_for_the_ci() {
    let (skipped, _) = ci_with_a_dependabot_run(json!(true));
    assert_eq!(skipped["result"], json!({ "outcome": "skipped", "state": "none" }), "{skipped}");
    let (followed, asked) = ci_with_a_dependabot_run(json!(false));
    assert_eq!(followed["result"], json!({ "outcome": "passed", "state": "passed" }), "{followed}");
    assert!(!of_kind(&asked, "wait.poll").is_empty());
}

#[test]
fn checks_without_any_command_say_so_instead_of_an_empty_log() {
    let text = r#"
import { workflow } from "sdk";
import { checks } from "sdk/steps";
export default workflow({
  name: "so-checks",
  async run(ctx) {
    const checked = await checks(ctx, { commands: [] });
    return { outcome: checked.outcome, output: checked.data.output };
  },
});
"#;
    let (result, asked) = drive(&source("workflows/so-checks.ts", text), &item(json!("T-1")), |_| None);
    assert!(of_kind(&asked, "exec").is_empty());
    assert_eq!(result["result"]["outcome"], "passed", "{result}");
    assert!(
        result["result"]["output"].as_str().is_some_and(|out| out.contains("Nenhum comando de verificação")),
        "{result}"
    );
}

const WRITTEN_PLAN: &str = "Vou ler o código antes.\n# Cadastro de clientes\n\n## Descrição\nComo atendente, eu quero cadastrar clientes para que eu os atenda depois.\n\n## Critérios de aceite\n- Dado que estou no cadastro, quando salvo um cliente, então ele aparece na lista.\n\n## Subtarefas\n### Criar a tabela\nTabela clientes com nome e documento.\n\n### Criar a ferramenta\nFerramenta MCP de cadastro.";

#[test]
fn the_plan_gives_the_title_the_story_goes_to_the_issue_and_the_pull_request_is_short() {
    let mut fake = Fake::default();
    let (result, asked) = drive(&default_workflow(), &item(Value::Null), |command| {
        let name = command["args"]["name"].as_str();
        if command["kind"] == "agent" && name == Some("planner") {
            return Some(json!({ "text": WRITTEN_PLAN, "session": "plan-1", "output": null }));
        }
        let mut reply = fake.reply(command)?;
        if command["kind"] == "agent" && name == Some("developer") {
            reply["text"] = json!("Mudei muita coisa.\n\nResumo: Cadastra clientes pela ferramenta MCP.");
        }
        Some(reply)
    });
    assert_eq!(result["state"], "completed", "{result}");
    let created = calls(&asked, "create_issue");
    let issue = &created[0]["args"]["args"];
    assert_eq!(issue["title"], "Cadastro de clientes");
    let story = issue["description"].as_str().expect("description");
    assert!(
        story.starts_with("Como atendente, eu quero")
            && story.contains("## Critérios de aceite\n- Dado que")
            && !story.contains("Criar a tabela"),
        "{story}"
    );
    let subtasks: Vec<(&Value, &Value, &Value)> = created[1..]
        .iter()
        .map(|call| {
            (&call["args"]["args"]["title"], &call["args"]["args"]["description"], &call["args"]["args"]["parent"])
        })
        .collect();
    assert_eq!(
        subtasks,
        [
            (&json!("Criar a tabela"), &json!("Tabela clientes com nome e documento."), &json!("T-9")),
            (&json!("Criar a ferramenta"), &json!("Ferramenta MCP de cadastro."), &json!("T-9")),
        ]
    );
    let developer =
        of_kind(&asked, "agent").into_iter().find(|command| command["args"]["name"] == "developer").cloned();
    let prompt = developer.expect("developer")["args"]["input"]["prompt"].as_str().unwrap_or_default().to_owned();
    assert!(prompt.contains("Dado que estou no cadastro") && prompt.contains("Criar a tabela"), "{prompt}");
    let opened = &calls(&asked, "open_pull_request")[0]["args"]["args"];
    assert_eq!(opened["title"], "T-9 - Cadastro de clientes");
    assert_eq!(opened["body"], "Cadastra clientes pela ferramenta MCP.\n\nIssue: T-9");
}

#[test]
fn a_verifier_that_fails_the_delivery_fails_the_checks() {
    let text = r#"
import { workflow } from "sdk";
import { checks } from "sdk/steps";
export default workflow({
  name: "so-checks",
  async run(ctx) {
    try {
      await checks(ctx, { commands: [], agent: "verifier" });
      return { outcome: "passed" };
    } catch (error) {
      return { outcome: "failed", error: error.outcome.data.error };
    }
  },
});
"#;
    let (result, asked) = drive(&source("workflows/so-checks.ts", text), &item(json!("T-1")), |command| {
        (command["kind"] == "agent").then(|| json!({ "text": "✗ cadastro\nVEREDITO: REPROVADO", "session": "ver-1" }))
    });
    assert_eq!(of_kind(&asked, "agent")[0]["args"]["name"], "verifier");
    assert_eq!(
        result["result"],
        json!({ "outcome": "failed", "error": "o verificador reprovou a entrega" }),
        "{result}"
    );
}

#[test]
fn a_developer_that_stops_without_a_commit_is_told_to_go_on_in_the_same_session() {
    let mut fake = Fake::default();
    let mut developed = 0;
    let (result, asked) = drive(&default_workflow(), &item(json!("T-1")), |command| {
        let mut reply = fake.reply(command)?;
        if command["kind"] == "agent" && command["args"]["name"] == "developer" {
            developed += 1;
            reply["base"] = json!("b0");
            reply["commit"] = json!(if developed == 1 { "b0" } else { "c1" });
        }
        Some(reply)
    });
    assert_eq!(result["state"], "completed", "{result}");
    let developers: Vec<&Value> =
        of_kind(&asked, "agent").into_iter().filter(|command| command["args"]["name"] == "developer").collect();
    let nudge = &developers[1]["args"]["input"];
    assert!(
        nudge["prompt"].as_str().is_some_and(|prompt| prompt.starts_with("Você encerrou sem nenhum commit")),
        "{nudge}"
    );
    assert_eq!(nudge["session"], "dev-1");
}

#[test]
fn a_develop_run_that_left_no_commit_fails_there_instead_of_at_the_pull_request() {
    let mut fake = Fake::default();
    let (result, asked) = drive(&default_workflow(), &item(json!("T-1")), |command| {
        let mut reply = fake.reply(command)?;
        if command["kind"] == "agent" && command["args"]["name"] == "developer" {
            reply["commit"] = Value::Null;
            reply["base"] = Value::Null;
        }
        Some(reply)
    });
    assert_eq!(result["state"], "cancelled", "{result}");
    assert!(calls(&asked, "open_pull_request").is_empty());
    let reasons: Vec<&str> =
        of_kind(&asked, "impediment.set").iter().filter_map(|command| command["args"]["reason"].as_str()).collect();
    assert!(
        reasons
            .iter()
            .any(|reason| reason.starts_with("Develop falhou") && reason.contains("não é um repositório git")),
        "{reasons:?}"
    );
}

#[test]
fn a_known_issue_is_reused_and_a_rejected_deploy_raises_an_impediment() {
    let mut fake = Fake { reject_deploy: true, ..Fake::default() };
    let (result, asked) = drive(&default_workflow(), &item(json!("T-1")), |command| fake.reply(command));
    assert_eq!(result["state"], "cancelled", "{result}");
    assert!(calls(&asked, "create_issue").is_empty());
    let developer =
        of_kind(&asked, "agent").into_iter().find(|command| command["args"]["name"] == "developer").cloned();
    assert!(developer.expect("developer")["args"]["input"]["issue"] == "T-1");
    let decision = &calls(&asked, "review_deployment")[0]["args"]["args"]["decision"];
    assert_eq!(decision, "reject");
    let impediments: Vec<&Value> = of_kind(&asked, "impediment.set").iter().map(|command| &command["args"]).collect();
    assert_eq!(impediments[0]["on"], true);
    assert!(impediments[0]["reason"].as_str().is_some_and(|reason| reason.contains("producao")));
    assert_eq!(impediments[0], &json!({ "on": true, "reason": "o deploy em producao foi rejeitado" }));
    assert!(
        impediments.iter().any(|args| args["on"] == true
            && args["reason"].as_str().is_some_and(|reason| reason.starts_with("Release falhou")))
    );
    assert!(!of_kind(&asked, "status.set").iter().any(|command| command["args"]["name"] == "Feito"));
    assert!(
        !of_kind(&asked, "agent").iter().any(|command| command["args"]["name"] == "release"),
        "a rejected deploy needs a person, not release notes"
    );
}

#[test]
fn each_step_view_is_made_of_blocks() {
    let source = default_workflow();
    let view = |step: &str, state: Value| call(&source, "host.view", &[json!({ "step": step, "state": state })]);
    assert_eq!(
        view("plan", json!({ "session": "s1" })),
        json!([{ "type": "chat", "session": "s1", "composer": true }])
    );
    assert_eq!(view("ticket", json!({ "issue": "T-1" })), json!([{ "type": "issue", "ref": "T-1" }]));
    assert_eq!(
        view("develop", json!({ "session": "s2" })),
        json!([{ "type": "chat", "session": "s2", "composer": false }])
    );
    assert_eq!(view("checks", json!({ "output": "erro" })), json!([{ "type": "logs", "text": "erro" }]));
    let ran = json!([
        { "name": "bun run lint", "state": "done", "seconds": null, "log": "ok" },
        { "name": "bun test", "state": "done", "seconds": null, "log": "12 pass" },
    ]);
    let verified =
        json!({ "output": "x", "ran": ran, "session": "v1", "verdict": "✓ a\nVEREDITO: APROVADO", "approved": true });
    let shown = view("checks", verified);
    let names: Vec<&Value> =
        shown[0]["commands"].as_array().expect("commands").iter().map(|command| &command["name"]).collect();
    assert_eq!(names, [&json!("bun run lint"), &json!("bun test"), &json!("Verificador: critérios de aceite")]);
    assert_eq!(shown[0]["commands"][2]["state"], "done");
    assert_eq!(shown[1], json!({ "type": "chat", "session": "v1", "composer": false }));
    let running = view("checks", json!({ "session": "v1" }));
    assert_eq!(running[0]["commands"][0]["state"], "running", "the verifier is still at it");
    assert_eq!(view("pr", json!({ "title": "T", "body": "B" })), json!([{ "type": "markdown", "text": "# T\n\nB" }]));
    assert_eq!(view("ci", json!({ "run": "R" })), json!([{ "type": "pipeline", "run": "R" }]));
    assert_eq!(view("review", json!({ "pr": "P" })), json!([{ "type": "review", "pr": "P" }]));
    assert_eq!(
        view("review", json!({ "pr": "P", "session": "r1" })),
        json!([{ "type": "review", "pr": "P", "session": "r1" }])
    );
    assert_eq!(
        view("merge", json!({ "pr": "P", "methods": ["squash"] })),
        json!([{ "type": "merge", "pr": "P", "methods": ["squash"] }])
    );
    assert_eq!(view("release", json!({ "run": "R" })), json!([{ "type": "pipeline", "run": "R" }]));
    assert_eq!(view("plan", json!({})), json!([]));
    let prs = json!({ "prs": [{ "pr": "P1", "name": "a/web", "title": "T", "body": "" }, { "pr": "P2", "name": "a/api", "title": "T", "body": "" }] });
    assert_eq!(view("pr", prs), json!([{ "type": "change", "pr": "P1" }, { "type": "change", "pr": "P2" }]));
    let drafts = json!({ "prs": [{ "name": "a/web", "title": "T", "body": "" }, { "name": "a/api", "title": "T", "body": "" }] });
    let shown: Vec<Value> =
        view("pr", drafts).as_array().expect("blocks").iter().map(|block| block["text"].clone()).collect();
    assert_eq!(shown, [json!("# a/web: T\n\n"), json!("# a/api: T\n\n")]);
    assert_eq!(
        view("pr", json!({ "pr": "o/r#3", "title": "T", "body": "B" })),
        json!([{ "type": "change", "pr": "o/r#3" }])
    );
    let runs = json!({ "run": "R1", "runs": [{ "run": "R1" }, { "run": "R2" }] });
    assert_eq!(
        view("ci", runs.clone()),
        json!([{ "type": "pipeline", "run": "R1" }, { "type": "pipeline", "run": "R2" }])
    );
    assert_eq!(view("release", runs).as_array().map(Vec::len), Some(2));
    assert_eq!(
        view("release", json!({ "run": "R", "notes": "## Notas", "session": "s3" })),
        json!([
            { "type": "pipeline", "run": "R" },
            { "type": "markdown", "text": "## Notas" },
            { "type": "chat", "session": "s3", "composer": false }
        ])
    );
    let merges = json!({ "pr": "P1", "methods": ["squash"], "merges": [{ "pr": "P1" }, { "pr": "P2" }] });
    assert_eq!(view("merge", merges).as_array().map(Vec::len), Some(2));
    assert_eq!(view("review", json!({ "pr": "P1", "prs": ["P1", "P2"] })).as_array().map(Vec::len), Some(2));
}

fn application_item() -> Value {
    let mut chosen = item(Value::Null);
    chosen["application"] = json!({ "id": "app", "name": "Loja", "repositories": [
        { "id": "web", "directory": "/src/web", "code": { "connection": "cw", "repo": "acme/web", "pipelines": true },
          "run": "bun dev", "verify": ["lint", "test"] },
        { "id": "api", "directory": "/src/api", "code": { "connection": "cw", "repo": "acme/api", "pipelines": true },
          "verify": ["cargo test"] },
        { "id": "docs", "directory": "/src/docs", "verify": [] },
    ] });
    chosen
}

fn developed(args: &Value) -> Value {
    let mut done = agent(args);
    if args["name"] == "developer" {
        done["repositories"] = json!([
            { "id": "web", "changed": true }, { "id": "api", "changed": true }, { "id": "docs", "changed": true },
        ]);
    }
    done
}

#[test]
fn an_application_with_two_coded_repositories_verifies_each_and_delivers_a_pull_request_per_repository() {
    let mut fake = Fake::default();
    let (result, asked) = drive(&default_workflow(), &application_item(), |command| match command["kind"].as_str() {
        Some("agent") => Some(developed(&command["args"])),
        _ => fake.reply(command),
    });
    assert_eq!(result["state"], "completed", "{result}");
    let execs: Vec<(String, String)> = of_kind(&asked, "exec")
        .iter()
        .map(|command| {
            (
                command["args"]["repository"].as_str().unwrap_or_default().to_owned(),
                command["args"]["command"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    assert!(
        execs.contains(&("web".into(), "lint".into())) && execs.contains(&("api".into(), "cargo test".into())),
        "{execs:?}"
    );
    let opened: Vec<(&Value, &Value)> = calls(&asked, "open_pull_request")
        .iter()
        .map(|command| (&command["args"]["args"]["repo"], &command["connection"]))
        .collect();
    assert_eq!(opened, [(&json!("acme/web"), &json!("cw")), (&json!("acme/api"), &json!("cw"))]);
    let runs: Vec<&Value> = calls(&asked, "runs").iter().map(|command| &command["args"]["args"]["repo"]).collect();
    assert!(runs.contains(&&json!("acme/web")) && runs.contains(&&json!("acme/api")), "{runs:?}");
    assert_eq!(calls(&asked, "merge").len(), 2);
    assert_eq!(calls(&asked, "submit_review").len(), 3, "the changes asked on the first, then both approved");
}

fn custom(labels: Value, event: bool) -> (Value, Vec<Value>) {
    let source = source("workflows/rapido.js", CUSTOM);
    let mut fake = Fake::default();
    let item = json!({ "id": "i2", "workflow": "rapido", "title": "y", "labels": labels, "branch": "feature/x", "issue": null });
    drive(&source, &item, |command| match command["kind"].as_str() {
        Some("child") => Some(json!({ "findings": 0 })),
        Some("event") if event => Some(json!({ "kind": "tracker.status.changed" })),
        Some("wait.human") if event => None,
        Some("wait.human") => Some(json!({ "action": "ok", "values": {}, "by": "ana" })),
        _ => fake.reply(command),
    })
}

#[test]
fn a_custom_workflow_composes_steps_with_conditions_branches_and_races() {
    let (result, asked) = custom(json!([]), false);
    assert_eq!(result["state"], "completed", "{result}");
    assert_eq!(result["result"], json!({ "scan": { "findings": 0 }, "checks": "$ make test\nok\n", "qa": "ok" }));
    let statuses: Vec<&Value> = of_kind(&asked, "status.set").iter().map(|command| &command["args"]["name"]).collect();
    assert_eq!(statuses, ["Backlog", "Em curso", "Pronto"]);
    assert_eq!(of_kind(&asked, "agent")[0]["args"]["name"], "planner");
    let forks: Vec<&Value> =
        result["signals"].as_array().expect("signals").iter().filter(|signal| signal["kind"] == "fork").collect();
    assert_eq!(forks.iter().map(|fork| fork["mode"].as_str()).collect::<Vec<_>>(), [Some("all"), Some("race")]);
    let child = of_kind(&asked, "child")[0];
    assert_eq!(child["args"], json!({ "workflow": "varredura", "input": { "ref": "feature/x" } }));
    assert_eq!(child["branch"], forks[0]["branches"][1]);
    assert_eq!(of_kind(&asked, "exec")[0]["branch"], forks[0]["branches"][0]);
    let qa = of_kind(&asked, "wait.human")[0];
    assert_eq!((qa["step"].as_str(), &qa["branch"]), (Some("qa"), &forks[1]["branches"][1]));
    let event = of_kind(&asked, "event")[0];
    assert_eq!(result["cancel"], json!([event["seq"]]), "the losing event is cancelled");

    let (hotfix, asked) = custom(json!(["hotfix"]), false);
    assert_eq!(hotfix["state"], "completed", "{hotfix}");
    assert!(
        of_kind(&asked, "agent").iter().all(|command| command["args"]["name"] == "developer"),
        "a hotfix skips the plan"
    );

    let (cancelled, asked) = custom(json!(["hotfix"]), true);
    assert_eq!(cancelled["state"], "cancelled", "{cancelled}");
    assert_eq!(cancelled["reason"], "cancelado no tracker");
    assert_eq!(cancelled["cancel"], json!([of_kind(&asked, "wait.human")[0]["seq"]]));
}

#[test]
fn test_cases_are_written_reviewed_by_a_person_and_created_linked_to_the_issue() {
    let mut fake = Fake::default();
    let mut answers = 0;
    let (result, asked) = drive(&default_workflow(), &item(Value::Null), |command| {
        let offers = command["args"]["view"]
            .as_array()
            .is_some_and(|view| view.iter().any(|block| block["approve"] == "approve_cases"));
        if command["kind"] == "wait.human" && offers {
            answers += 1;
            if answers == 1 {
                return Some(
                    json!({ "action": "submit", "values": { "changes": "inclua o caso de tela" }, "by": "ana" }),
                );
            }
        }
        fake.reply(command)
    });
    assert_eq!(result["state"], "completed", "{result}");
    let testers: Vec<&Value> =
        of_kind(&asked, "agent").into_iter().filter(|command| command["args"]["name"] == "tester").collect();
    assert_eq!(testers.len(), 4, "draft, redraft after the changes, then an E2E run per passing checks");
    assert_eq!(testers[3]["args"]["input"]["session"], "test-2", "the E2E run goes on in its own session");
    assert_eq!(testers[0]["args"]["mode"], "plan");
    assert_eq!(testers[1]["args"]["input"]["session"], "test-1");
    assert!(
        testers[1]["args"]["input"]["prompt"].as_str().is_some_and(|prompt| prompt.contains("inclua o caso de tela"))
    );
    let created = &calls(&asked, "create_case")[0]["args"]["args"];
    assert_eq!(created["title"], "Cadastrar cliente pela API");
    assert_eq!(created["labels"], json!(["e2e", "api"]));
    assert_eq!(created["issue"], "T-9");
    assert_eq!(created["issue_id"], "10009");
    assert_eq!(created["precondition"], "Dado nenhum cliente Ana");
    let step = json!({ "action": "Quando envio POST /clientes com nome=Ana", "expected": "Então a resposta tem status 201\nE Ana fica gravada" });
    assert_eq!(created["steps"], json!([step]));
    assert_eq!(calls(&asked, "create_cycle")[0]["args"]["args"]["issue_id"], "10009");
    let run = testers[2]["args"]["input"]["prompt"].as_str().expect("prompt");
    assert!(run.contains("Z-T1") && run.contains("Cadastrar cliente pela API"), "{run}");
    assert_eq!(testers[2]["args"]["worktree"], "item/i1");
    let recorded = &calls(&asked, "record")[0]["args"]["args"];
    assert_eq!(
        *recorded,
        json!({ "case": "Z-T1", "cycle": "Z-R1", "status": "passed", "comment": "api.spec.ts passou" })
    );
}

const E2E_ONLY: &str = r#"
import { workflow } from "sdk";
import { checks } from "sdk/steps";
export default workflow({
  name: "so-e2e",
  async run(ctx) {
    try {
      const checked = await checks(ctx, { commands: [] });
      return { outcome: checked.outcome, data: checked.data };
    } catch (error) {
      return { outcome: "failed", data: error.outcome.data };
    }
  },
});
"#;

#[test]
fn checks_run_the_e2e_commands_and_a_failing_one_fails_with_the_command() {
    let mut chosen = item(json!("T-1"));
    chosen["e2e"] = json!(["npx playwright test"]);
    let (result, asked) = drive(&source("workflows/so-e2e.ts", E2E_ONLY), &chosen, |command| {
        Some(match command["kind"].as_str() {
            Some("exec") => json!({ "code": 1, "output": "1 failed", "stdout": "", "stderr": "" }),
            _ => Value::Null,
        })
    });
    assert_eq!(of_kind(&asked, "exec")[0]["args"]["command"], "npx playwright test");
    assert!(of_kind(&asked, "agent").is_empty(), "without created cases only the commands run");
    assert_eq!(result["result"]["outcome"], "failed", "{result}");
    let error = result["result"]["data"]["error"].as_str().expect("error");
    assert!(error.contains("npx playwright test"), "{error}");
}

const CASES_THEN_CHECKS: &str = r#"
import { workflow } from "sdk";
import { cases, checks } from "sdk/steps";
export default workflow({
  name: "casos",
  steps: [cases({ approve: "auto" }), checks({ commands: [], tester: "tester" })],
});
"#;

#[test]
fn a_case_the_tester_saw_fail_is_recorded_and_fails_the_checks_with_its_evidence() {
    let mut fake = Fake::default();
    let (result, asked) = drive(&source("workflows/casos.ts", CASES_THEN_CHECKS), &item(json!("T-1")), |command| {
        let prompt = command["args"]["input"]["prompt"].as_str().unwrap_or_default();
        if command["kind"] == "agent" && prompt.starts_with("Automatize") {
            let results = json!([{ "case": "Z-T1", "status": "failed", "evidence": "esperava 201, veio 500" }]);
            return Some(json!({ "text": "", "session": "test-2", "output": { "results": results } }));
        }
        fake.reply(command)
    });
    let views: Vec<String> = of_kind(&asked, "wait.human").iter().map(|command| command["args"].to_string()).collect();
    assert!(views.iter().all(|view| !view.contains("approve_cases")), "approve auto skips the person: {views:?}");
    assert_ne!(result["state"], "completed", "{result} {views:?}");
    assert_eq!(calls(&asked, "record")[0]["args"]["args"]["status"], "failed");
    let text = result.to_string();
    assert!(text.contains("Z-T1") && text.contains("esperava 201, veio 500"), "{text}");
}

const ONLY_TICKET: &str = r#"
import { workflow } from "sdk";
import { ticket } from "sdk/steps";
export default workflow({
  name: "so-ticket",
  async run(ctx) {
    const done = await ticket(ctx, {});
    return { outcome: done.outcome, data: done.data };
  },
});
"#;

#[test]
fn a_ticket_without_any_tracker_is_skipped_with_a_reason_instead_of_blocking() {
    let (result, asked) = drive(&source("workflows/so-ticket.ts", ONLY_TICKET), &item(Value::Null), |command| {
        match (command["kind"].as_str(), command["optional"].as_bool()) {
            (Some("call"), Some(true)) => Some(Value::Null),
            _ => None,
        }
    });
    let created = calls(&asked, "create_issue");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].get("connection"), None);
    assert_eq!(result["result"]["outcome"], "skipped", "{result}");
    let reason = result["result"]["data"]["reason"].as_str().expect("a reason");
    assert!(reason.contains("rastreador"), "{reason}");
    let shown = step_view("ticket", result["result"]["data"].clone());
    assert_eq!(shown[0]["type"], "markdown", "{shown}");
}

#[test]
fn a_ticket_with_a_chosen_tracker_still_blocks_when_it_is_missing() {
    let mut fake = Fake::default();
    let mut chosen = item(Value::Null);
    chosen["connection"] = json!("c9");
    let (result, asked) = drive(&source("workflows/so-ticket.ts", ONLY_TICKET), &chosen, |command| fake.reply(command));
    let created = calls(&asked, "create_issue");
    assert_eq!(created[0]["connection"], "c9");
    assert_eq!(created[0].get("optional"), None, "a chosen connection is never optional");
    assert_eq!(result["result"]["data"]["issue"], "T-9", "{result}");
}

const ONLY_PR: &str = r#"
import { workflow } from "sdk";
import { pr } from "sdk/steps";
export default workflow({
  name: "so-pr",
  async run(ctx) {
    const done = await pr(ctx, {});
    return { outcome: done.outcome, data: done.data };
  },
});
"#;

fn step_view(step: &str, state: Value) -> Value {
    call(&default_workflow(), "host.view", &[json!({ "step": step, "state": state })])
}

fn one_repository_item() -> Value {
    let mut chosen = item(Value::Null);
    chosen["application"] = json!({ "id": "app", "name": "Loja", "repositories": [
        { "id": "web", "directory": "/src/web", "code": { "connection": "cw", "repo": "acme/web", "pipelines": true } },
    ] });
    chosen
}

#[test]
fn the_pull_request_opens_from_the_item_s_head_into_its_base() {
    let mut fake = Fake::default();
    let mut chosen = one_repository_item();
    chosen["git"] = json!({ "web": { "base": "release/1", "head": "fix/x" } });
    let (result, asked) = drive(&source("workflows/so-pr.ts", ONLY_PR), &chosen, |command| fake.reply(command));
    let opened = calls(&asked, "open_pull_request");
    assert_eq!(opened[0]["args"]["args"]["base"], "release/1");
    assert_eq!(opened[0]["args"]["args"]["head"], "fix/x");
    assert_eq!(result["result"]["data"]["base"], "main", "{result}");
}

#[test]
fn a_pull_request_with_a_remote_default_base_sends_no_base() {
    let mut fake = Fake::default();
    let mut chosen = one_repository_item();
    chosen["git"] = json!({ "web": { "base": null, "head": "fix/x" } });
    let (_, asked) = drive(&source("workflows/so-pr.ts", ONLY_PR), &chosen, |command| fake.reply(command));
    let opened = calls(&asked, "open_pull_request");
    assert_eq!(opened[0]["args"]["args"].get("base"), None);
}

fn pull_request_item() -> Value {
    let mut chosen = one_repository_item();
    chosen["input"] = json!({ "sources": [
        { "kind": "pull_request", "connection": "cx", "subject": "acme/web#42", "repository": "web" },
    ] });
    chosen["git"] = json!({ "web": { "base": "main", "head": "feature/y" } });
    chosen["branch"] = json!("feature/y");
    chosen
}

#[test]
fn an_item_started_from_a_pull_request_reuses_it_instead_of_opening_another() {
    let (result, asked) =
        drive(&source("workflows/so-pr.ts", ONLY_PR), &pull_request_item(), |command| match command["args"]["action"]
            .as_str()
        {
            Some("pull_request") => Some(json!({
                "ref": "acme/web#42", "number": 42, "title": "Ajusta y", "body": "", "url": "https://code/42",
                "head": "feature/y", "base": "main"
            })),
            _ => Some(Value::Null),
        });
    assert!(calls(&asked, "open_pull_request").is_empty(), "the pull request already exists");
    let read = calls(&asked, "pull_request");
    assert_eq!(read[0]["args"]["args"]["pr"], "acme/web#42");
    assert_eq!(read[0]["connection"], "cx");
    let data = &result["result"]["data"];
    assert_eq!((&data["pr"], &data["number"], &data["reused"]), (&json!("acme/web#42"), &json!(42), &json!(true)));
}

fn pull_request_workflow() -> String {
    let text = super::builtin_text("workflows/pull-request.ts").expect("the pull request workflow is built in");
    source("workflows/pull-request.ts", &text)
}

#[test]
fn the_workflows_say_which_sources_they_accept() {
    let described = call(&pull_request_workflow(), "host.describe", &[]);
    let workflow = &described["workflows"][0];
    assert_eq!(workflow["accepts"], json!(["pull_request"]));
    let ids: Vec<&str> =
        workflow["steps"].as_array().expect("steps").iter().filter_map(|call| call["id"].as_str()).collect();
    assert_eq!(ids, ["develop", "checks", "ci", "review", "merge", "release"]);
    let default = call(&default_workflow(), "host.describe", &[]);
    assert_eq!(default["workflows"][0]["accepts"], json!(["issue", "text"]));
}

#[test]
fn a_pull_request_item_fixes_its_open_comments_then_carries_the_same_pull_request_to_merge() {
    let mut fake = Fake::default();
    let (result, asked) = drive(&pull_request_workflow(), &pull_request_item(), |command| fake.reply(command));
    assert_eq!(result["state"], "completed", "{result}");
    assert!(calls(&asked, "open_pull_request").is_empty());
    let merged = calls(&asked, "merge");
    assert_eq!((&merged[0]["args"]["args"]["pr"], &merged[0]["connection"]), (&json!("acme/web#42"), &json!("cx")));
    let prompts: Vec<&str> = of_kind(&asked, "agent")
        .iter()
        .filter(|command| command["args"]["name"] == "developer")
        .filter_map(|command| command["args"]["input"]["prompt"].as_str())
        .collect();
    assert!(prompts.first().is_some_and(|prompt| prompt.contains("trate o erro")), "{prompts:?}");
    let ci: Vec<&Value> = calls(&asked, "runs").iter().map(|command| &command["args"]["args"]["ref"]).collect();
    assert!(ci.contains(&&json!("feature/y")), "{ci:?}");
}

#[test]
fn a_pull_request_without_pending_work_skips_the_developer() {
    let text = r#"
import { workflow } from "sdk";
import { develop } from "sdk/steps";
export default workflow({
  name: "so-develop",
  async run(ctx) {
    const done = await develop(ctx, { agent: "developer", feedbackOnly: true });
    return { outcome: done.outcome, data: done.data };
  },
});
"#;
    let (result, asked) =
        drive(&source("workflows/so-develop.ts", text), &pull_request_item(), |command| {
            match command["kind"].as_str() {
                Some("call") => Some(json!([{ "id": "c1", "body": "ok", "resolved": true }])),
                _ => Some(Value::Null),
            }
        });
    assert!(of_kind(&asked, "agent").is_empty());
    assert_eq!(result["result"]["outcome"], "skipped", "{result}");
    assert_eq!(step_view("develop", result["result"]["data"].clone())[0]["type"], "markdown");
}

#[test]
fn cases_go_to_the_test_project_chosen_in_the_application() {
    let mut fake = Fake::default();
    let mut chosen = item(json!("T-1"));
    chosen["application"] = json!({ "tests": { "connection": "z1", "project": "QA" }, "repositories": [] });
    let (_, asked) = drive(&source("workflows/casos.ts", CASES_THEN_CHECKS), &chosen, |command| fake.reply(command));
    let created = &calls(&asked, "create_case")[0];
    assert_eq!(created["args"]["args"]["project"], "QA");
    assert_eq!(created["connection"], "z1");
    let cycle = &calls(&asked, "create_cycle")[0];
    assert_eq!(cycle["args"]["args"]["project"], "QA");
    assert_eq!(cycle["args"]["args"]["issue"], "T-1");
}
