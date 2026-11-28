use serde_json::{Value, json};

use super::{drive, of_kind, source};

const ROUTED: &str = r#"
import { workflow, role, admins } from "sdk";
import { approve, requestInfo, handoff } from "sdk/steps";

export default workflow({
  name: "compra",
  steps: [
    requestInfo({ text: "Qual o centro de custo?", fields: [{ name: "centro", label: "Centro", kind: "text", required: true }], assignTo: role("compras") }),
    approve({ text: (item) => `Aprovar ${item.title}`, assignTo: role("controller") }),
    handoff({ text: "Emitir o pedido", assignTo: admins() }),
  ],
});
"#;

fn item() -> Value {
    json!({ "id": "c1", "workflow": "compra", "title": "Notebook", "labels": [], "branch": null, "issue": null, "input": null })
}

#[derive(Default)]
struct Desk {
    asked: usize,
    rejected: bool,
}

impl Desk {
    fn reply(&mut self, command: &Value) -> Option<Value> {
        if command["kind"] != "wait.human" {
            return Some(Value::Null);
        }
        self.asked += 1;
        Some(match command["step"].as_str()? {
            "request-info" if self.asked == 1 => {
                json!({ "action": "submit", "values": { "centro": " " }, "by": "ana" })
            }
            "request-info" => json!({ "action": "submit", "values": { "centro": "CC-12" }, "by": "ana" }),
            "approve" if self.rejected => json!({ "action": "reject", "values": { "note": "caro" }, "by": "bia" }),
            "approve" => json!({ "action": "approve", "values": { "note": "ok, dentro do teto" }, "by": "bia" }),
            _ => json!({ "action": "done", "values": {}, "by": "root" }),
        })
    }
}

#[test]
fn each_human_step_waits_for_its_assignee_and_keeps_who_answered_and_the_note() {
    let mut desk = Desk::default();
    let (result, asked) = drive(&source("workflows/compra.ts", ROUTED), &item(), |command| desk.reply(command));
    assert_eq!(result["state"], "completed", "{result}");
    let waits = of_kind(&asked, "wait.human");
    let assigned: Vec<&Value> = waits.iter().map(|command| &command["args"]["assignTo"]).collect();
    assert_eq!(
        assigned,
        [
            &json!({ "role": "compras" }),
            &json!({ "role": "compras" }),
            &json!({ "role": "controller" }),
            &json!("admins")
        ]
    );
    assert_eq!(waits[2]["args"]["view"][0]["text"], "Aprovar Notebook");
    let form = &waits[0]["args"]["view"][1];
    assert_eq!(form["type"], "form", "{form}");
    let rendered = result.to_string();
    assert!(rendered.contains("CC-12") && rendered.contains("ok, dentro do teto"), "{rendered}");
}

#[test]
fn a_rejection_fails_the_approval_and_offers_the_usual_way_out() {
    let mut desk = Desk { rejected: true, ..Desk::default() };
    let (result, asked) = drive(&source("workflows/compra.ts", ROUTED), &item(), |command| {
        if command["kind"] == "wait.human" && command["step"].is_null() {
            return Some(json!({ "action": "cancel", "values": {}, "by": "bia" }));
        }
        desk.reply(command)
    });
    assert_eq!(result["state"], "cancelled", "{result}");
    let waits = of_kind(&asked, "wait.human");
    let last = waits.last().map(|command| command["args"]["view"][0]["text"].clone()).unwrap_or_default();
    assert_eq!(last, "**Approval failed.** rejected");
}

#[test]
fn a_wait_without_an_assignee_sends_none() {
    let plain = r#"
import { workflow } from "sdk";
import { approve } from "sdk/steps";
export default workflow({ name: "simples", steps: [approve()] });
"#;
    let mut desk = Desk::default();
    let (result, asked) = drive(&source("workflows/simples.ts", plain), &item(), |command| desk.reply(command));
    assert_eq!(result["state"], "completed", "{result}");
    assert!(of_kind(&asked, "wait.human").iter().all(|command| command["args"].get("assignTo").is_none()));
}
