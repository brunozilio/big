use serde_json::{Value, json};

use super::{drive, of_kind, source};

const ANSWER: &str = r#"
import { workflow } from "sdk";
import { draft, review, act, deliver } from "sdk/steps/general.js";

export default workflow({
  name: "resposta",
  steps: [
    draft({ agent: "drafter", fields: (item) => ({ cliente: "Ana", pedido: item.title }) }),
    review({ onChanges: "draft", approveLabel: "Aprovar" }),
    act({ capability: "email", action: "send", args: (item, ctx) => ({ to: "ana@cliente.test", subject: item.title, body: ctx.output("draft").draft }) }),
    deliver({ capability: "tracker", action: "comment", args: (item, ctx) => ({ issue: "SAC-12", body: ctx.output("draft").draft }) }),
  ],
});
"#;

fn answer_item() -> Value {
    json!({ "id": "r1", "workflow": "resposta", "title": "Pedido 881 chegou incompleto", "labels": [], "branch": null,
        "issue": null, "input": null })
}

struct Desk {
    drafts: usize,
    reviews: usize,
}

impl Desk {
    fn reply(&mut self, command: &Value) -> Option<Value> {
        match command["kind"].as_str()? {
            "agent" => {
                self.drafts += 1;
                let text = format!("Olá, Ana. Rascunho {}.", self.drafts);
                Some(json!({ "text": text, "session": format!("d-{}", self.drafts), "output": null }))
            }
            "wait.human" => {
                self.reviews += 1;
                if self.reviews == 1 {
                    return Some(
                        json!({ "action": "submit", "values": { "body": "Cite o prazo de reenvio" }, "by": "ana" }),
                    );
                }
                Some(json!({ "action": "approve", "values": {}, "by": "bia" }))
            }
            "call" => Some(json!({ "ok": true, "ref": "m-1" })),
            _ => Some(Value::Null),
        }
    }
}

#[test]
fn a_draft_reviewed_acted_on_and_delivered_runs_without_any_code_connector() {
    let mut desk = Desk { drafts: 0, reviews: 0 };
    let (result, asked) =
        drive(&source("workflows/resposta.ts", ANSWER), &answer_item(), |command| desk.reply(command));
    assert_eq!(result["state"], "completed", "{result}");
    let agents = of_kind(&asked, "agent");
    assert_eq!(agents.len(), 2, "the review sent the draft back once");
    assert!(
        agents.iter().all(|command| command["args"]["inPlace"] == json!(true) && command["args"]["worktree"].is_null())
    );
    let second = agents[1]["args"]["input"]["prompt"].as_str().unwrap_or_default();
    assert!(second.contains("Cite o prazo de reenvio") && second.contains("cliente: Ana"), "{second}");
    let calls = of_kind(&asked, "call");
    let used: Vec<(&str, &str)> = calls
        .iter()
        .filter_map(|command| Some((command["args"]["capability"].as_str()?, command["args"]["action"].as_str()?)))
        .collect();
    assert!(used.contains(&("email", "send")) && used.contains(&("tracker", "comment")), "{used:?}");
    assert!(used.iter().all(|(capability, _)| !["code", "pipelines"].contains(capability)));
    assert!(of_kind(&asked, "exec").is_empty(), "nothing runs in a repository");
    let sent = calls.iter().find(|command| command["args"]["action"] == "send").expect("sent");
    assert_eq!(sent["args"]["args"]["body"], "Olá, Ana. Rascunho 2.");
    let view = &of_kind(&asked, "wait.human")[0]["args"]["view"];
    assert!(view.as_array().is_some_and(|blocks| blocks.iter().any(|block| block["text"] == "Olá, Ana. Rascunho 1.")));
}
