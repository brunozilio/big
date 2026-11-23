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

const STEPS: [&str; 9] = ["plan", "ticket", "develop", "checks", "pr", "ci", "review", "merge", "release"];

const PROVIDERS: [&str; 7] = ["jira", "github", "aws", "atlassian", "gitlab", "bitbucket", "amazon"];

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
                Some(error) => json!({ "seq": command["seq"], "kind": command["kind"], "error": error, "at": 2000 + round }),
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
        _ => json!({ "text": "", "session": "other", "output": null }),
    }
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
            "workflows/default.ts",
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
    assert!(super::builtin_files("sdk/steps/").len() >= paths.len() - 1);
    let index = super::builtin_text("sdk/steps/index.js").expect("index");
    assert!(STEPS.iter().all(|step| index.contains(&format!("default as {step} "))));
}

#[test]
fn the_default_workflow_describes_its_statuses_and_the_nine_steps_in_order() {
    let described = call(&default_workflow(), "host.describe", &[]);
    let workflow = &described["workflows"][0];
    assert_eq!(workflow["name"], "entrega");
    assert_eq!(workflow["statuses"], json!(["A fazer", "Fazendo", "Revisando", "Feito"]));
    let ids: Vec<&str> =
        workflow["steps"].as_array().expect("steps").iter().filter_map(|call| call["id"].as_str()).collect();
    assert_eq!(ids, STEPS);
    assert_eq!(workflow["steps"][3]["options"]["onFail"], "develop");
    assert_eq!(workflow["steps"][3]["options"]["agent"], "verifier");
    assert_eq!(workflow["steps"][5]["options"]["onFail"], "develop");
    assert_eq!(workflow["steps"][6]["options"]["onChanges"], "develop");
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
    let created = calls(&asked, "create_issue");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0]["args"]["args"], json!({ "title": "Corrigir x", "description": "1. mudar x" }), "no format, the whole plan");
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
    assert_eq!(polled, ["CI-1", "CI-2", "REL-1", "REL-1", "REL-1"], "a new run per CI attempt and the run raced with the approval");
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
    assert!(reviewers.iter().all(|command| command["args"]["mode"] == "plan" && command["args"]["input"]["pr"] == "PR-7"));
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
    let (result, asked) = drive(&source("workflows/so-ci.ts", text), &item(json!("T-1")), |command| match command["kind"].as_str() {
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
    let (result, asked) = drive(&source("workflows/so-ci.ts", text), &item(json!("T-1")), |command| match command["kind"].as_str() {
        Some("call") if command["args"]["args"].get("ref").is_none() => Some(json!([{ "ref": "old", "state": "passed" }])),
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
    let (result, asked) = drive(&source("workflows/so-ci.ts", text), &item(json!("T-1")), |command| match command["kind"].as_str() {
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
    assert!(result["result"]["output"].as_str().is_some_and(|out| out.contains("Nenhum comando de verificação")), "{result}");
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
    assert!(story.starts_with("Como atendente, eu quero") && story.contains("## Critérios de aceite\n- Dado que") && !story.contains("Criar a tabela"), "{story}");
    let subtasks: Vec<(&Value, &Value, &Value)> = created[1..]
        .iter()
        .map(|call| (&call["args"]["args"]["title"], &call["args"]["args"]["description"], &call["args"]["args"]["parent"]))
        .collect();
    assert_eq!(subtasks, [
        (&json!("Criar a tabela"), &json!("Tabela clientes com nome e documento."), &json!("T-9")),
        (&json!("Criar a ferramenta"), &json!("Ferramenta MCP de cadastro."), &json!("T-9")),
    ]);
    let developer = of_kind(&asked, "agent").into_iter().find(|command| command["args"]["name"] == "developer").cloned();
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
    assert_eq!(result["result"], json!({ "outcome": "failed", "error": "o verificador reprovou a entrega" }), "{result}");
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
    assert!(reasons.iter().any(|reason| reason.starts_with("Develop falhou") && reason.contains("não é um repositório git")), "{reasons:?}");
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
        impediments
            .iter()
            .any(|args| args["on"] == true && args["reason"].as_str().is_some_and(|reason| reason.starts_with("Release falhou")))
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
    assert_eq!(view("plan", json!({ "session": "s1" })), json!([{ "type": "chat", "session": "s1", "composer": true }]));
    assert_eq!(view("ticket", json!({ "issue": "T-1" })), json!([{ "type": "issue", "ref": "T-1" }]));
    assert_eq!(view("develop", json!({ "session": "s2" })), json!([{ "type": "chat", "session": "s2", "composer": false }]));
    assert_eq!(view("checks", json!({ "output": "erro" })), json!([{ "type": "logs", "text": "erro" }]));
    let ran = json!([
        { "name": "bun run lint", "state": "done", "seconds": null, "log": "ok" },
        { "name": "bun test", "state": "done", "seconds": null, "log": "12 pass" },
    ]);
    let verified = json!({ "output": "x", "ran": ran, "session": "v1", "verdict": "✓ a\nVEREDITO: APROVADO", "approved": true });
    let shown = view("checks", verified);
    let names: Vec<&Value> = shown[0]["commands"].as_array().expect("commands").iter().map(|command| &command["name"]).collect();
    assert_eq!(names, [&json!("bun run lint"), &json!("bun test"), &json!("Verificador: critérios de aceite")]);
    assert_eq!(shown[0]["commands"][2]["state"], "done");
    assert_eq!(shown[1], json!({ "type": "chat", "session": "v1", "composer": false }));
    let running = view("checks", json!({ "session": "v1" }));
    assert_eq!(running[0]["commands"][0]["state"], "running", "the verifier is still at it");
    assert_eq!(view("pr", json!({ "title": "T", "body": "B" })), json!([{ "type": "markdown", "text": "# T\n\nB" }]));
    assert_eq!(view("ci", json!({ "run": "R" })), json!([{ "type": "pipeline", "run": "R" }]));
    assert_eq!(view("review", json!({ "pr": "P" })), json!([{ "type": "review", "pr": "P" }]));
    assert_eq!(
        view("merge", json!({ "pr": "P", "methods": ["squash"] })),
        json!([{ "type": "merge", "pr": "P", "methods": ["squash"] }])
    );
    assert_eq!(view("release", json!({ "run": "R" })), json!([{ "type": "pipeline", "run": "R" }]));
    assert_eq!(view("plan", json!({})), json!([]));
    let prs = json!({ "prs": [{ "pr": "P1", "name": "a/web", "title": "T", "body": "" }, { "pr": "P2", "name": "a/api", "title": "T", "body": "" }] });
    assert_eq!(view("pr", prs), json!([{ "type": "change", "pr": "P1" }, { "type": "change", "pr": "P2" }]));
    let drafts = json!({ "prs": [{ "name": "a/web", "title": "T", "body": "" }, { "name": "a/api", "title": "T", "body": "" }] });
    let shown: Vec<Value> = view("pr", drafts).as_array().expect("blocks").iter().map(|block| block["text"].clone()).collect();
    assert_eq!(shown, [json!("# a/web: T\n\n"), json!("# a/api: T\n\n")]);
    assert_eq!(view("pr", json!({ "pr": "o/r#3", "title": "T", "body": "B" })), json!([{ "type": "change", "pr": "o/r#3" }]));
    let runs = json!({ "run": "R1", "runs": [{ "run": "R1" }, { "run": "R2" }] });
    assert_eq!(view("ci", runs.clone()), json!([{ "type": "pipeline", "run": "R1" }, { "type": "pipeline", "run": "R2" }]));
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
        .map(|command| (command["args"]["repository"].as_str().unwrap_or_default().to_owned(), command["args"]["command"].as_str().unwrap_or_default().to_owned()))
        .collect();
    assert!(execs.contains(&("web".into(), "lint".into())) && execs.contains(&("api".into(), "cargo test".into())), "{execs:?}");
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
