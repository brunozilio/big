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

const PATHS: [&str; 3] = ["workflows/security.ts", "sdk/steps/findings.js", "agents/security-fixer.ts"];

const PROVIDERS: [&str; 5] = ["veracode", "snyk", "sonar", "github", "jira"];

fn workflow() -> String {
    let text = super::builtin_text("workflows/security.ts").expect("the security workflow is built in");
    bundle("workflows/security.ts", &text).unwrap_or_else(|error| panic!("bundle: {error}"))
}

fn call(source: &str, function: &str, arguments: &[Value]) -> Value {
    let host: Arc<dyn script::Host> = Arc::new(NoHost);
    let stop = Arc::new(AtomicBool::new(false));
    script::run("workflow.js", source, function, arguments, &Value::Null, host, stop)
        .unwrap_or_else(|error| panic!("{function}: {error}"))
}

fn drive(item: &Value, mut reply: impl FnMut(&Value) -> Value) -> (Value, Vec<Value>) {
    let source = workflow();
    let mut journal: Vec<Value> = Vec::new();
    let mut asked: Vec<Value> = Vec::new();
    for round in 0..200 {
        let activation = json!({ "item": item, "journal": journal, "start": 1000, "seed": "i1" });
        let result = call(&source, "host.activate", &[activation]);
        if result["state"] != "running" {
            return (result, asked);
        }
        for command in result["pending"].as_array().cloned().unwrap_or_default() {
            let value = reply(&command);
            journal.push(json!({ "seq": command["seq"], "kind": command["kind"], "ok": value, "at": 2000 + round }));
            asked.push(command);
        }
    }
    panic!("the workflow never ended")
}

fn finding(id: u64, severity: u64) -> Value {
    json!({
        "id": id.to_string(), "title": "SQL Injection", "severity": severity, "kind": "static", "status": "open",
        "new": true, "policy": true, "cwe": { "id": "89", "name": "SQL Injection" }, "cve": null,
        "file": "src/db.rs", "line": 42, "module": null, "component": null, "version": null,
        "description": "Unsanitized input reaches a query.", "found": null, "url": null
    })
}

fn item() -> Value {
    json!({
        "id": "i1", "workflow": "security", "title": "Vulnerabilidades · Loja · 2026-11-24", "labels": [],
        "language": "pt-BR",
        "schedule": { "day": "2026-11-24" },
        "application": {
            "id": "app", "name": "Loja",
            "bindings": { "tracker": { "connection": "trk", "value": "LOJA" } },
            "repositories": [{
                "id": "r1", "directory": "/src/loja",
                "bindings": {
                    "code": { "connection": "code", "value": "acme/loja" },
                    "security": { "connection": "sec", "value": "guid-1" }
                }
            }]
        }
    })
}

struct Fake {
    found: Value,
    open_body: String,
}

impl Fake {
    fn reply(&self, command: &Value) -> Value {
        let args = &command["args"];
        match command["kind"].as_str().unwrap_or_default() {
            "agent" => json!({ "text": "Summary: corrigido", "session": "s1", "output": { "branch": "item/i1" } }),
            "wait.human" => {
                let offered = args["view"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|block| block["buttons"].as_array().cloned().unwrap_or_default());
                let action =
                    if offered.into_iter().any(|button| button["id"] == "cancel") { "cancel" } else { "approve" };
                json!({ "action": action, "values": {}, "by": "ana" })
            }
            "call" => self.call(args),
            _ => Value::Null,
        }
    }

    fn call(&self, args: &Value) -> Value {
        let inner = &args["args"];
        match (args["capability"].as_str(), args["action"].as_str()) {
            (Some("security"), Some("findings")) => self.found.clone(),
            (Some("code"), Some("pull_requests")) => json!([{ "ref": "acme/loja#3", "body": self.open_body }]),
            (Some("tracker"), Some("projects")) => json!([{ "id": "1", "key": "LOJA", "name": "Loja" }]),
            (Some("tracker"), Some("create_issue")) => {
                json!({ "ref": format!("LOJA-{}", inner["title"].as_str().unwrap_or_default().len()), "status": "A fazer" })
            }
            (Some("tracker"), Some("statuses")) => json!(["A fazer", "Fazendo", "Feito"]),
            (Some("code"), Some("open_pull_request")) => {
                json!({ "ref": "acme/loja#9", "number": 9, "body": inner["body"], "head": inner["head"] })
            }
            (Some("code"), Some("review_comments")) => json!([]),
            _ => json!({ "ok": true }),
        }
    }
}

fn actions<'a>(asked: &'a [Value], action: &str) -> Vec<&'a Value> {
    asked.iter().filter(|command| command["kind"] == "call" && command["args"]["action"] == action).collect()
}

#[test]
fn the_security_workflow_is_built_in_never_scheduled_and_names_no_provider() {
    for path in PATHS {
        let text = super::builtin_text(path).unwrap_or_else(|| panic!("{path} is built in"));
        let lower = text.to_lowercase();
        assert!(PROVIDERS.iter().all(|provider| !lower.contains(provider)), "{path} names a provider");
    }
    let described = call(&workflow(), "host.describe", &[]);
    let shape = &described["workflows"][0];
    assert_eq!(shape["name"], "security");
    assert_eq!(shape["triggers"], json!([]));
    let ids: Vec<&str> =
        shape["steps"].as_array().expect("steps").iter().filter_map(|step| step["id"].as_str()).collect();
    assert_eq!(ids, ["plan", "ticket", "develop", "checks", "pr", "review"]);
}

#[test]
fn new_findings_become_an_issue_with_subtasks_a_fix_and_a_pull_request_that_marks_them() {
    let fake = Fake {
        found: json!([finding(11, 5), finding(12, 4), finding(13, 3)]),
        open_body: "Antigo\n\n<!-- security-findings: 12 -->".into(),
    };
    let (result, asked) = drive(&item(), |command| fake.reply(command));
    assert_eq!(result["state"], "completed", "{result}");
    let read = actions(&asked, "findings");
    assert_eq!(read[0]["connection"], "sec");
    assert_eq!(read[0]["args"]["args"]["project"], "guid-1");
    assert_eq!(read[0]["args"]["args"]["minSeverity"], 3);
    assert_eq!(actions(&asked, "pull_requests")[0]["connection"], "code");
    let issues = actions(&asked, "create_issue");
    assert_eq!(issues[0]["args"]["args"]["title"], "Corrigir vulnerabilidades · 2026-11-24");
    assert_eq!(issues.len(), 3, "the issue and one subtask per new finding");
    let fixing = asked.iter().find(|command| command["kind"] == "agent" && command["args"]["name"] == "security-fixer");
    let prompt = fixing.expect("the fixer runs")["args"]["input"]["prompt"].as_str().unwrap_or_default().to_owned();
    assert!(prompt.contains("src/db.rs:42"), "{prompt}");
    let opened = actions(&asked, "open_pull_request");
    let body = opened[0]["args"]["args"]["body"].as_str().unwrap_or_default();
    assert!(body.contains("<!-- security-findings: 11,13 -->"), "{body}");
    assert!(!body.contains("| 12 |"), "{body}");
}

#[test]
fn a_day_without_new_findings_ends_right_after_reading_them() {
    let fake = Fake { found: json!([finding(12, 4)]), open_body: "<!-- security-findings: 12 -->".into() };
    let (result, asked) = drive(&item(), |command| fake.reply(command));
    assert_eq!(result["state"], "completed", "{result}");
    assert_eq!(result["result"]["findings"], json!([]));
    assert_eq!(result["result"]["held"], json!(["12"]));
    assert!(actions(&asked, "create_issue").is_empty());
    assert!(asked.iter().all(|command| command["kind"] != "agent"));
}

#[test]
fn an_application_without_a_security_project_fails_with_what_to_do() {
    let mut unset = item();
    unset["application"]["repositories"][0]["bindings"]["security"] = Value::Null;
    let fake = Fake { found: json!([]), open_body: String::new() };
    let (_, asked) = drive(&unset, |command| fake.reply(command));
    let shown = asked.iter().find(|command| command["kind"] == "wait.human").expect("the failure waits for a person");
    let text = shown["args"]["view"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains("Configurações → Aplicações"), "{text}");
}
