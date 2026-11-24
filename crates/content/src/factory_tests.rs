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

fn source(path: &str) -> String {
    let text = super::builtin_text(path).unwrap_or_else(|| panic!("{path} is built in"));
    bundle(path, &text).unwrap_or_else(|error| panic!("bundle: {error}"))
}

fn call(source: &str, activation: &Value) -> Value {
    let host: Arc<dyn script::Host> = Arc::new(NoHost);
    let stop = Arc::new(AtomicBool::new(false));
    script::run("workflow.js", source, "host.activate", std::slice::from_ref(activation), &Value::Null, host, stop)
        .unwrap_or_else(|error| panic!("activate: {error}"))
}

fn drive(source: &str, target: &Value, item: &Value, mut reply: impl FnMut(&Value) -> Value) -> (Value, Vec<Value>) {
    let mut journal: Vec<Value> = Vec::new();
    let mut asked: Vec<Value> = Vec::new();
    for round in 0..200 {
        let activation = json!({ "target": target, "item": item, "journal": journal, "start": 1000, "seed": "i1" });
        let result = call(source, &activation);
        if result["state"] != "running" {
            return (result, asked);
        }
        for command in result["pending"].as_array().expect("pending") {
            if asked.iter().any(|known| known["seq"] == command["seq"]) {
                continue;
            }
            asked.push(command.clone());
            let value = reply(command);
            journal.push(json!({ "seq": command["seq"], "kind": command["kind"], "ok": value, "at": 2000 + round }));
        }
    }
    panic!("the workflow never ended")
}

fn item(autonomy: &str, findings: Value) -> Value {
    json!({
        "id": "i1", "workflow": "factory-evals", "title": "Avaliação", "labels": [], "branch": "item/i1", "issue": null,
        "factory": {
            "kind": "factory.gate.escaped", "subject": "orig:checks:1:escape:review:1", "autonomy": autonomy,
            "origin": "orig", "base": "/home/u/.big/factories/acme", "chain": ["acme"], "dir": ".big",
            "data": { "workflow": "entrega", "step": "develop", "gate": "checks", "caught_by": "review", "findings": findings }
        }
    })
}

fn finding() -> Value {
    json!({ "id": "orig:checks:1:escape:review:1", "kind": "escape", "gate": "checks", "caught_by": "review",
        "detail": { "caught": { "body": "quebrou o login" } } })
}

const CASE: &str =
    r#"{ "name": "login", "origin": "orig:checks:1:escape:review:1", "commit": "bad1", "command": "test login" }"#;

#[derive(Default)]
struct Host {
    detector_catches: bool,
    listed: String,
}

impl Host {
    fn reply(&self, command: &Value) -> Value {
        let args = &command["args"];
        match command["kind"].as_str().unwrap_or_default() {
            "agent" => json!({ "text": "Resumo: caso novo", "session": "s1", "output": { "branch": "item/i1" } }),
            "exec" => self.exec(args),
            "wait.human" => json!({ "action": "approve", "values": {}, "by": "ana" }),
            "child" => json!({ "tuned": true }),
            _ => Value::Null,
        }
    }

    fn exec(&self, args: &Value) -> Value {
        let command = args["command"].as_str().unwrap_or_default();
        let env = &args["env"];
        let (code, out) = if env["EVALS"].is_string() {
            (0, self.listed.clone())
        } else if env["COMMIT"].is_string() {
            (i32::from(self.detector_catches), "rodou no commit".to_owned())
        } else if command.contains("merge-base") {
            (0, "base0\n".to_owned())
        } else if env["TARGET"].is_string() {
            (0, "evals/login.json\n".to_owned())
        } else {
            (0, "ok".to_owned())
        };
        json!({ "code": code, "output": out, "stdout": out, "stderr": "" })
    }
}

fn of_kind<'a>(asked: &'a [Value], kind: &str) -> Vec<&'a Value> {
    asked.iter().filter(|command| command["kind"] == kind).collect()
}

#[test]
fn the_meta_loop_writes_a_case_checks_it_promotes_it_into_the_base_factory_and_hands_off_the_gate() {
    let source = source("workflows/factory-evals.ts");
    let host = Host { detector_catches: true, listed: format!(".big/evals/login.json\t{CASE}\n") };
    let (result, asked) =
        drive(&source, &json!({ "workflow": "factory-evals" }), &item("near", json!([finding()])), |command| {
            host.reply(command)
        });
    assert_eq!(result["state"], "completed", "{result}");
    let agents = of_kind(&asked, "agent");
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0]["args"]["name"], "eval-engineer");
    let prompt = agents[0]["args"]["input"]["prompt"].as_str().unwrap_or_default();
    assert!(prompt.contains("quebrou o login") && prompt.contains(".big/evals/"), "{prompt}");
    assert_eq!(of_kind(&asked, "wait.human").len(), 1, "near the loop, a human approves only the promotion");
    let execs = of_kind(&asked, "exec");
    assert!(execs.iter().any(|command| command["args"]["env"]["COMMIT"] == "bad1"));
    assert!(
        execs.iter().any(|command| command["args"]["command"] == "test login"),
        "the detector passes on the current code"
    );
    let inherit = execs.iter().find(|command| command["args"]["env"]["TARGET"].is_string()).expect("inherited");
    assert_eq!(inherit["args"]["env"]["TARGET"], "/home/u/.big/factories/acme");
    assert_eq!(inherit["args"]["env"]["BASE"], "base0");
    let child = of_kind(&asked, "child");
    assert_eq!(child.len(), 1);
    assert_eq!(child[0]["args"]["workflow"], "factory-tune");
    assert_eq!(child[0]["args"]["input"]["factory"]["data"]["step"], "checks");
}

#[test]
fn rejecting_the_promotion_cancels_the_item_without_merging() {
    let source = source("workflows/factory-evals.ts");
    let host = Host { detector_catches: true, listed: format!(".big/evals/login.json\t{CASE}\n") };
    let (result, asked) =
        drive(&source, &json!({ "workflow": "factory-evals" }), &item("near", json!([finding()])), |command| {
            if command["kind"] == "wait.human" {
                return json!({ "action": "reject", "values": {}, "by": "ana" });
            }
            host.reply(command)
        });
    assert_eq!(result["state"], "cancelled", "{result}");
    let execs = of_kind(&asked, "exec");
    assert!(
        !execs.iter().any(|command| command["args"]["command"].as_str().unwrap_or_default().contains("merge --no-ff"))
    );
    assert!(of_kind(&asked, "child").is_empty());
}

#[test]
fn an_eval_fails_when_the_detector_misses_the_defect_or_the_case_for_the_finding_is_missing() {
    let source = source("sdk/factory/index.js");
    let target = json!({ "step": "evals", "options": { "requireOrigin": true } });
    let missing = Host { detector_catches: true, listed: String::new() };
    let (result, _) = drive(&source, &target, &item("near", json!([finding()])), |command| missing.reply(command));
    assert_eq!(result["result"]["outcome"], "failed", "{result}");
    assert_eq!(result["result"]["data"]["missing"], json!(["orig:checks:1:escape:review:1"]));
    let blind = Host { detector_catches: false, listed: format!("f\t{CASE}\n") };
    let (result, _) = drive(&source, &target, &item("near", json!([finding()])), |command| blind.reply(command));
    assert_eq!(result["result"]["outcome"], "failed");
    let good = Host { detector_catches: true, listed: format!("f\t{CASE}\nbroken\t{{\n") };
    let (result, _) = drive(&source, &target, &item("near", json!([finding()])), |command| good.reply(command));
    assert_eq!(result["result"]["outcome"], "failed", "a broken case file fails the evals");
}

#[test]
fn autonomy_decides_where_the_human_stands() {
    let source = source("workflows/factory-evals.ts");
    let host = Host { detector_catches: true, listed: format!("f\t{CASE}\n") };
    let target = json!({ "workflow": "factory-evals" });
    let (result, asked) = drive(&source, &target, &item("out", json!([finding()])), |command| host.reply(command));
    assert_eq!(result["state"], "completed");
    assert!(of_kind(&asked, "wait.human").is_empty(), "out of the loop, the evals are the only gate");
    let (_, asked) = drive(&source, &target, &item("in", json!([finding()])), |command| host.reply(command));
    let waits = of_kind(&asked, "wait.human");
    assert_eq!(waits.len(), 2, "in the loop, a human approves the brief and the promotion");
    assert!(waits[0]["args"]["view"].to_string().contains("quebrou o login"));
}

#[test]
fn the_factory_library_names_no_provider_and_has_no_comments() {
    for (path, bytes) in super::builtin_files("sdk/factory/") {
        let text = String::from_utf8_lossy(bytes).to_lowercase();
        for provider in ["jira", "github", "aws", "atlassian", "gitlab"] {
            assert!(!text.contains(provider), "{path} names {provider}");
        }
        assert!(!text.contains("//") || text.contains("://"), "{path} has no comments");
    }
    assert!(super::builtin_text("agents/factory-engineer.ts").is_some());
    assert!(super::builtin_text("agents/eval-engineer.ts").is_some());
}
