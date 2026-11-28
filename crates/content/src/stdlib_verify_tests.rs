use serde_json::{Value, json};

use super::{drive, of_kind, source};

const PATH: &str = "workflows/invoice-match.ts";

fn matching() -> String {
    let text = crate::builtin_text(PATH).expect("the example is built in");
    let plain = script::typescript::plain(PATH, &text).expect("it compiles");
    source(PATH, &plain)
}

fn invoice_item(number: &str) -> Value {
    json!({ "id": format!("nf{number}"), "workflow": "nf-pedido", "title": format!("Conferir NF {number}"), "labels": [],
        "branch": null, "issue": null, "input": null })
}

fn party() -> Value {
    json!({ "document": "12.345.678/0001-90", "name": "Metalúrgica Andrade Ltda" })
}

fn invoice(number: &str, sheet: f64) -> Value {
    json!({ "number": number, "series": "1", "issuer": party(), "order": "4500012345", "total": 840.0 + sheet * 10.0,
        "lines": [
            { "code": "PAR-M8", "description": "Parafuso sextavado M8 x 30", "quantity": 2000, "unit_price": 0.42 },
            { "code": "CHP-3", "description": "Chapa de aço 3 mm", "quantity": 10, "unit_price": sheet }
        ] })
}

fn order(sheet: f64) -> Value {
    json!({ "number": "4500012345", "status": "approved", "tolerance": 0.02, "supplier": party(), "total": 840.0 + sheet * 10.0,
        "lines": [
            { "code": "PAR-M8", "description": "Parafuso sextavado M8 x 30", "quantity": 2000, "unit_price": 0.42, "received": 2000 },
            { "code": "CHP-3", "description": "Chapa de aço 3 mm", "quantity": 10, "unit_price": sheet, "received": 10 }
        ] })
}

struct Purchasing {
    invoice: Value,
    orders: Vec<Value>,
    read: usize,
}

impl Purchasing {
    fn reply(&mut self, command: &Value) -> Option<Value> {
        let args = &command["args"];
        match command["kind"].as_str()? {
            "call" => Some(match args["action"].as_str()? {
                "invoice" => self.invoice.clone(),
                "order" => {
                    self.read += 1;
                    self.orders[(self.read - 1).min(self.orders.len() - 1)].clone()
                }
                "match" => json!({ "invoice": args["args"]["invoice"], "status": "matched" }),
                "release" => json!({ "invoice": args["args"]["invoice"], "status": "released", "due": "2026-12-26" }),
                _ => json!({}),
            }),
            "wait.human" => {
                let buttons: Vec<Value> = args["view"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|block| block["buttons"].as_array().cloned().unwrap_or_default())
                    .collect();
                let action =
                    if buttons.iter().any(|button| button["id"] == "continue") { "continue" } else { "approve" };
                Some(json!({ "action": action, "values": {}, "by": "ana" }))
            }
            _ => Some(Value::Null),
        }
    }
}

fn actions(asked: &[Value]) -> Vec<&str> {
    of_kind(asked, "call").iter().filter_map(|command| command["args"]["action"].as_str()).collect()
}

#[test]
fn an_invoice_that_matches_its_order_is_approved_by_a_person_and_released() {
    let mut books = Purchasing { invoice: invoice("4521", 389.9), orders: vec![order(389.9)], read: 0 };
    let (result, asked) = drive(&matching(), &invoice_item("4521"), |command| books.reply(command));
    assert_eq!(result["state"], "completed", "{result}");
    assert_eq!(actions(&asked), ["invoice", "order", "match", "release"]);
    let asked_invoice = &of_kind(&asked, "call")[0]["args"]["args"]["invoice"];
    assert_eq!(asked_invoice, "4521");
    let humans = of_kind(&asked, "wait.human");
    assert_eq!(humans.len(), 1, "green criteria are a floor: a person still approves the payment");
    let shown = humans[0]["args"]["view"].to_string();
    assert!(shown.contains("✓ Preço") && shown.contains("✓ Mesmo fornecedor"), "{shown}");
    assert!(of_kind(&asked, "exec").is_empty() && of_kind(&asked, "agent").is_empty(), "no repository check, no agent");
}

#[test]
fn a_price_off_the_tolerance_waits_for_a_fix_and_is_checked_again() {
    let mut books = Purchasing { invoice: invoice("4533", 412.5), orders: vec![order(389.9), order(412.5)], read: 0 };
    let (result, asked) = drive(&matching(), &invoice_item("4533"), |command| books.reply(command));
    assert_eq!(result["state"], "completed", "{result}");
    assert_eq!(actions(&asked), ["invoice", "order", "invoice", "order", "match", "release"]);
    let first = of_kind(&asked, "wait.human")[0]["args"]["view"].to_string();
    assert!(first.contains("Conferir de novo"), "{first}");
}

#[test]
fn an_invoice_that_never_matches_stops_after_its_retries_and_asks_a_person() {
    let mut books = Purchasing { invoice: invoice("4533", 412.5), orders: vec![order(389.9)], read: 0 };
    let mut decided = false;
    let (result, asked) = drive(&matching(), &invoice_item("4533"), |command| {
        let view = command["args"]["view"].to_string();
        if command["kind"] == "wait.human" && view.contains("\"cancel\"") {
            decided = true;
            return Some(json!({ "action": "cancel", "values": {}, "by": "ana" }));
        }
        books.reply(command)
    });
    assert!(decided, "the retries ran out and a person decides");
    assert_eq!(result["state"], "cancelled", "{result}");
    assert_eq!(actions(&asked).iter().filter(|action| **action == "order").count(), 3, "first try and two retries");
    assert!(!actions(&asked).contains(&"release"));
}
