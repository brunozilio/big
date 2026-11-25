use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::{CASES_THEN_CHECKS, Fake, ONLY_TICKET, calls, default_workflow, drive, item, of_kind, source, step_view};

fn views(asked: &[Value]) -> Vec<&Value> {
    of_kind(asked, "wait.human").into_iter().map(|command| &command["args"]["view"]).collect()
}

fn block<'a>(view: &'a Value, kind: &str) -> Option<&'a Value> {
    view.as_array().and_then(|blocks| blocks.iter().find(|block| block["type"] == kind))
}

fn projects() -> Value {
    json!([{ "id": "1", "key": "OPS", "name": "Operações" }, { "id": "2", "key": "WEB", "name": "Web" }])
}

#[test]
fn a_ticket_with_several_projects_asks_where_to_create_the_issue_with_a_form() {
    let mut fake = Fake::default();
    let mut answers = 0;
    let (result, asked) = drive(&source("workflows/so-ticket.ts", ONLY_TICKET), &item(Value::Null), |command| {
        if command["kind"] == "call" && command["args"]["action"] == "projects" {
            return Some(projects());
        }
        if command["kind"] == "wait.human" {
            answers += 1;
            let project = if answers == 1 { "NADA" } else { "WEB" };
            return Some(json!({ "action": "submit", "values": { "project": project, "type": " Bug " }, "by": "ana" }));
        }
        fake.reply(command)
    });
    assert_eq!(result["result"]["outcome"], "passed", "{result}");
    let shown = views(&asked);
    assert_eq!(shown.len(), 2, "an unknown project asks again");
    let asked_form = block(shown[0], "form").expect("a form");
    assert_eq!(asked_form["title"], "Where to create the issue");
    assert_eq!(asked_form["submit"], "Create issue");
    assert_eq!(asked_form["fields"][0]["kind"], "select");
    assert_eq!(asked_form["fields"][0]["options"][0], json!({ "value": "OPS", "label": "OPS · Operações" }));
    assert_eq!(asked_form["fields"][1]["name"], "type");
    let created = &calls(&asked, "create_issue")[0]["args"]["args"];
    assert_eq!(created["project"], "WEB");
    assert_eq!(created["type"], "Bug");
    assert_eq!(result["result"]["data"]["project"], "WEB");
}

#[test]
fn a_ticket_with_one_project_uses_it_without_asking() {
    let mut fake = Fake::default();
    let (result, asked) = drive(&source("workflows/so-ticket.ts", ONLY_TICKET), &item(Value::Null), |command| {
        if command["kind"] == "call" && command["args"]["action"] == "projects" {
            return Some(json!([{ "id": "1", "key": "OPS", "name": "Operações" }]));
        }
        fake.reply(command)
    });
    assert_eq!(result["result"]["data"]["issue"], "T-9", "{result}");
    assert!(views(&asked).is_empty());
    assert_eq!(calls(&asked, "create_issue")[0]["args"]["args"]["project"], "OPS");
}

const CHECKED: &str = r#"
import { workflow } from "sdk";
import { ticket } from "sdk/steps";
export default workflow({
  name: "conferido",
  statuses: ["A fazer", "Pronto"],
  steps: [ticket({ doneStatus: "Entregue" })],
});
"#;

fn checked(statuses: Value, answer: &'static [&'static str]) -> (Value, Vec<Value>) {
    let mut fake = Fake::default();
    let mut chosen = item(Value::Null);
    chosen["project"] = json!("OPS");
    let mut answered = 0;
    drive(&source("workflows/conferido.ts", CHECKED), &chosen, |command| {
        if command["kind"] == "call" && command["args"]["action"] == "statuses" {
            return Some(statuses.clone());
        }
        if command["kind"] == "wait.human" {
            let action = answer.get(answered).copied().unwrap_or("keep_statuses");
            answered += 1;
            return Some(json!({ "action": action, "values": {}, "by": "ana" }));
        }
        fake.reply(command)
    })
}

#[test]
fn statuses_the_tracker_does_not_have_raise_an_impediment_that_lists_the_real_ones() {
    let real = json!([{ "id": "1", "name": "a fazer" }, { "id": "2", "name": "Fazendo" }]);
    let (result, asked) = checked(real, &["recheck_statuses", "keep_statuses"]);
    assert_eq!(result["state"], "completed", "{result}");
    let listed = calls(&asked, "statuses");
    assert_eq!(listed.len(), 2, "checking again reads the tracker again");
    assert_eq!(listed[0]["args"]["args"], json!({ "project": "OPS" }));
    let impediments: Vec<&Value> = of_kind(&asked, "impediment.set").iter().map(|command| &command["args"]).collect();
    let reason = impediments[0]["reason"].as_str().expect("a reason");
    assert!(reason.contains("Pronto, Entregue") && reason.contains("a fazer, Fazendo"), "{reason}");
    assert!(!reason.contains("A fazer,"), "a status that differs only in case exists: {reason}");
    assert_eq!(impediments.last().map(|args| &args["on"]), Some(&json!(false)));
    let shown = views(&asked);
    let buttons = &block(shown[0], "actions").expect("actions")["buttons"];
    assert_eq!(buttons[0]["id"], "recheck_statuses");
    assert_eq!(buttons[1]["id"], "keep_statuses");
    let data = &result["result"]["data"];
    assert_eq!(data["unknownStatuses"], json!(["Pronto", "Entregue"]));
    let shown = step_view("ticket", data.clone());
    assert_eq!(shown[0]["type"], "issue");
    assert!(shown[1]["text"].as_str().is_some_and(|text| text.contains("Pronto")), "{shown}");
}

#[test]
fn statuses_the_tracker_has_need_nobody() {
    let real = json!([{ "name": "A fazer" }, { "name": "Pronto" }, { "name": "Entregue" }]);
    let (result, asked) = checked(real, &[]);
    assert_eq!(result["state"], "completed", "{result}");
    assert!(views(&asked).is_empty());
    assert!(of_kind(&asked, "impediment.set").is_empty());
    let (_, unknown) = checked(json!([]), &[]);
    assert!(views(&unknown).is_empty(), "a tracker that lists nothing is not checked");
}

fn required_fields() -> Value {
    json!([
        { "key": "summary", "name": "Resumo", "type": "string", "required": true },
        { "key": "customfield_1", "name": "Ambiente", "type": "option", "required": true,
          "values": [{ "id": "1", "name": "Produção" }, { "id": "2", "name": "Homologação" }] },
        { "key": "customfield_2", "name": "Cliente", "type": "string", "required": true },
    ])
}

#[test]
fn a_ticket_asks_for_the_required_fields_the_tracker_wants_and_sends_them_with_the_issue() {
    let mut fake = Fake::default();
    let mut chosen = item(Value::Null);
    chosen["project"] = json!("OPS");
    let mut answers = 0;
    let (result, asked) = drive(&source("workflows/so-ticket.ts", ONLY_TICKET), &chosen, |command| {
        if command["kind"] == "call" && command["args"]["action"] == "fields" {
            return Some(required_fields());
        }
        if command["kind"] == "wait.human" {
            answers += 1;
            let ambiente = if answers == 1 { "9" } else { "2" };
            return Some(
                json!({ "action": "submit", "values": { "customfield_1": ambiente, "customfield_2": " ACME " }, "by": "ana" }),
            );
        }
        fake.reply(command)
    });
    assert_eq!(result["result"]["outcome"], "passed", "{result}");
    assert_eq!(calls(&asked, "fields")[0]["args"]["args"], json!({ "project": "OPS" }));
    let shown = views(&asked);
    assert_eq!(shown.len(), 2, "a value outside the allowed ones asks again");
    let asked_form = block(shown[0], "form").expect("a form");
    assert_eq!(asked_form["title"], "Required fields");
    let fields = asked_form["fields"].as_array().expect("fields");
    assert_eq!(fields.len(), 2, "the summary is the step's own: {asked_form}");
    assert_eq!(fields[0]["kind"], "select");
    assert_eq!(fields[0]["options"][1], json!({ "value": "2", "label": "Homologação" }));
    assert_eq!((&fields[1]["name"], &fields[1]["label"]), (&json!("customfield_2"), &json!("Cliente")));
    assert_ne!(fields[1]["kind"], "select");
    let created = &calls(&asked, "create_issue")[0]["args"]["args"];
    assert_eq!(created["fields"], json!({ "customfield_1": "2", "customfield_2": "ACME" }));
}

#[test]
fn required_fields_the_workflow_already_gives_are_not_asked() {
    let mut fake = Fake::default();
    let mut chosen = item(Value::Null);
    chosen["project"] = json!("OPS");
    let given = ONLY_TICKET
        .replace("ticket(ctx, {})", "ticket(ctx, { fields: { customfield_1: \"1\", customfield_2: \"ACME\" } })");
    let (result, asked) = drive(&source("workflows/so-ticket.ts", &given), &chosen, |command| {
        if command["kind"] == "call" && command["args"]["action"] == "fields" {
            return Some(required_fields());
        }
        fake.reply(command)
    });
    assert_eq!(result["result"]["outcome"], "passed", "{result}");
    assert!(views(&asked).is_empty());
    assert_eq!(
        calls(&asked, "create_issue")[0]["args"]["args"]["fields"],
        json!({ "customfield_1": "1", "customfield_2": "ACME" })
    );
}

fn checked_issue(issue: &str, found: Value) -> Vec<Value> {
    let mut fake = Fake::default();
    let (result, asked) = drive(&source("workflows/conferido.ts", CHECKED), &item(json!(issue)), |command| {
        if command["kind"] == "call" && command["args"]["action"] == "statuses" {
            return Some(json!([{ "name": "A fazer" }]));
        }
        if command["kind"] == "call" && command["args"]["action"] == "issue" {
            return Some(found.clone());
        }
        if command["kind"] == "wait.human" {
            return Some(json!({ "action": "keep_statuses", "values": {}, "by": "ana" }));
        }
        fake.reply(command)
    });
    assert_eq!(result["state"], "completed", "{result}");
    assert!(calls(&asked, "create_issue").is_empty());
    asked
}

#[test]
fn an_item_started_from_an_issue_checks_the_statuses_of_the_issue_project() {
    let asked = checked_issue("WEB-12", Value::Null);
    assert!(calls(&asked, "issue").is_empty(), "the key already names the project");
    assert_eq!(calls(&asked, "statuses")[0]["args"]["args"], json!({ "project": "WEB" }));
    assert_eq!(of_kind(&asked, "impediment.set")[0]["args"]["on"], true);
    let asked = checked_issue(
        "10042",
        json!({ "ref": "APP-3", "title": "x", "status": "A fazer", "url": "", "project": "APP" }),
    );
    assert_eq!(calls(&asked, "issue")[0]["args"]["args"], json!({ "issue": "10042" }));
    assert_eq!(calls(&asked, "statuses")[0]["args"]["args"], json!({ "project": "APP" }));
}

fn linked_case() -> Value {
    json!([{ "ref": "Z-T7", "title": "Login pela tela", "precondition": "um usuário ativo" }])
}

fn drafting(args: &Value, cases: &Value) -> Option<Value> {
    let prompt = args["input"]["prompt"].as_str().unwrap_or_default();
    if prompt.starts_with("Automate") {
        return None;
    }
    Some(json!({ "text": "", "session": "test-1", "output": { "cases": cases } }))
}

fn reused(cases: Value) -> (Value, Vec<Value>) {
    let mut fake = Fake::default();
    drive(&source("workflows/casos.ts", CASES_THEN_CHECKS), &item(json!("T-1")), |command| {
        if command["kind"] == "call" && command["args"]["action"] == "cases" {
            return Some(linked_case());
        }
        if command["kind"] == "agent"
            && let Some(answer) = drafting(&command["args"], &cases)
        {
            return Some(answer);
        }
        fake.reply(command)
    })
}

#[test]
fn the_cases_already_linked_to_the_issue_are_reused_and_only_the_missing_ones_are_created() {
    let new =
        json!({ "title": "Cadastrar cliente pela API", "kind": "api", "given": [], "when": ["POST"], "then": ["201"] });
    let again = json!({ "title": "login pela TELA", "kind": "ui", "given": [], "when": ["entro"], "then": ["vejo"] });
    let (_, asked) = reused(json!([new, again]));
    let listed = calls(&asked, "cases");
    assert_eq!(listed[0]["args"]["args"], json!({ "issue": "T-1" }));
    assert_eq!(listed[0]["optional"], true);
    let testers: Vec<&Value> =
        of_kind(&asked, "agent").into_iter().filter(|command| command["args"]["name"] == "tester").collect();
    let draft = testers[0]["args"]["input"]["prompt"].as_str().expect("prompt");
    assert!(
        draft.contains("Z-T7") && draft.contains("Login pela tela") && draft.contains("Do not repeat them"),
        "{draft}"
    );
    let created: Vec<&Value> =
        calls(&asked, "create_case").iter().map(|command| &command["args"]["args"]["title"]).collect();
    assert_eq!(created, [&json!("Cadastrar cliente pela API")], "the linked case is not created again");
    let run = testers[1]["args"]["input"]["prompt"].as_str().expect("prompt");
    assert!(run.contains("Z-T7") && run.contains("Z-T1"), "the E2E run covers the reused and the new case: {run}");
}

#[test]
fn when_the_linked_cases_cover_everything_nothing_is_created_and_nobody_approves_an_empty_draft() {
    let (_, asked) = reused(json!([]));
    assert!(calls(&asked, "create_case").is_empty());
    assert_eq!(calls(&asked, "create_cycle").len(), 1);
    assert!(views(&asked).iter().all(|view| block(view, "cases").is_none()));
    let run = of_kind(&asked, "agent")
        .into_iter()
        .filter_map(|command| command["args"]["input"]["prompt"].as_str())
        .find(|prompt| prompt.starts_with("Automate"))
        .expect("the E2E run");
    assert!(run.contains("Z-T7"), "{run}");
}

const ONLY_CASES: &str = r#"
import { workflow } from "sdk";
import { cases } from "sdk/steps";
export default workflow({
  name: "so-casos",
  async run(ctx) {
    const done = await cases(ctx, { approve: "auto" });
    return { outcome: done.outcome, data: done.data };
  },
});
"#;

#[test]
fn a_missing_test_connection_tells_what_to_connect_and_lets_the_person_retry_or_skip() {
    let mut answered = Vec::new();
    let (result, asked) = drive(&source("workflows/so-casos.ts", ONLY_CASES), &item(json!("T-1")), |command| {
        match command["kind"].as_str() {
            Some("call") if command["args"]["action"] == "cases" => Some(Value::Null),
            Some("wait.human") => {
                answered.push(());
                let action = if answered.len() == 1 { "retry" } else { "skip" };
                Some(json!({ "action": action, "values": {}, "by": "ana" }))
            }
            Some("impediment.set") => Some(Value::Null),
            _ => None,
        }
    });
    assert_eq!(result["result"]["outcome"], "skipped", "{result}");
    assert_eq!(calls(&asked, "cases").len(), 2, "retrying looks for the connection again");
    assert!(of_kind(&asked, "agent").is_empty(), "the tester waits for the connection");
    let shown = views(&asked);
    let text = block(shown[0], "markdown").and_then(|found| found["text"].as_str()).expect("what is missing");
    assert!(text.contains("test cases") && text.contains("Connections"), "{text}");
    let ids: Vec<&Value> = block(shown[0], "actions").expect("actions")["buttons"]
        .as_array()
        .expect("buttons")
        .iter()
        .map(|b| &b["id"])
        .collect();
    assert_eq!(ids, ["retry", "skip", "cancel"]);
    let impediment = &of_kind(&asked, "impediment.set")[0]["args"];
    assert_eq!(impediment, &json!({ "on": true, "reason": "connect an account for tests" }));
    assert_eq!(step_view("cases", result["result"]["data"].clone())[0]["type"], "markdown");
}

const FIXED: &str = r#"
import { workflow } from "sdk";
import { develop, review } from "sdk/steps";
const feedback = { step: "review", outcome: "goto", data: { pr: "PR-7", comments: [
  { thread: "t1", path: "src/a.rs", line: 3, body: "trate o erro" },
  { thread: "t2", path: "src/b.rs", line: 1, body: "resolvido" },
  { thread: "t1", path: "src/a.rs", line: 3, body: "de novo" },
] } };
export default workflow({
  name: "corrigido",
  steps: [develop({ agent: "developer", feedback }), review({ pr: "PR-7" })],
});
"#;

#[test]
fn approving_resolves_only_the_handed_threads_that_are_still_open() {
    let mut fake = Fake { reviews: 1, ..Fake::default() };
    let (result, asked) =
        drive(&source("workflows/corrigido.ts", FIXED), &item(Value::Null), |command| fake.reply(command));
    assert_eq!(result["state"], "completed", "{result}");
    let resolved: Vec<&Value> =
        calls(&asked, "resolve_thread").iter().map(|command| &command["args"]["args"]).collect();
    assert_eq!(resolved, [&json!({ "thread": "t1", "pr": "PR-7" })], "t2 was already resolved");
    assert_eq!(result["result"]["data"]["resolved"], json!(["t1"]));
}

#[test]
fn every_block_kind_the_page_knows_is_emitted_by_a_standard_step() {
    let mut fake = Fake { reject_deploy: true, ..Fake::default() };
    let (_, asked) = drive(&default_workflow(), &item(Value::Null), |command| fake.reply(command));
    let mut seen: BTreeSet<String> = views(&asked)
        .into_iter()
        .flat_map(|view| view.as_array().cloned().unwrap_or_default())
        .filter_map(|block| block["type"].as_str().map(str::to_owned))
        .collect();
    let states = [
        ("plan", json!({ "session": "s1" })),
        ("ticket", json!({ "issue": "T-1" })),
        ("pr", json!({ "title": "T", "body": "B" })),
        ("checks", json!({ "output": "erro" })),
        ("ci", json!({ "run": "R" })),
    ];
    for (step, state) in states {
        let shown = step_view(step, state);
        seen.extend(
            shown.as_array().into_iter().flatten().filter_map(|block| block["type"].as_str().map(str::to_owned)),
        );
    }
    for kind in ["chat", "issue", "review", "pipeline", "merge", "markdown", "logs", "form", "actions"] {
        assert!(seen.contains(kind), "no standard step emits {kind}: {seen:?}");
    }
}
