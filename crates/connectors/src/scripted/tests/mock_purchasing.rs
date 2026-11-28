use std::sync::Arc;

use serde_json::{Value, json};

use super::{creds, loaded};
use crate::error::ActionError;
use crate::http::FakeHttp;
use crate::provider::Provider;
use crate::scripted::{Local, ScriptedProvider};

struct Books {
    provider: ScriptedProvider,
    ledger: Value,
}

impl Books {
    fn new() -> Books {
        Books { provider: loaded("mock_purchasing"), ledger: Value::Null }
    }

    fn run(&mut self, action: &str, args: &Value) -> Result<Value, ActionError> {
        let input = json!({ "action": action, "args": args, "ledger": self.ledger, "now": 1_795_000_000 });
        let out = self.provider.script.invoke_here("run", &input, Arc::new(Local))?;
        self.ledger = out["ledger"].clone();
        Ok(out["value"].clone())
    }
}

#[test]
fn an_invoice_cites_its_order_and_both_carry_lines_and_totals() {
    let mut books = Books::new();
    let invoice = books.run("purchasing.invoice", &json!({ "invoice": "4521" })).unwrap_or_default();
    assert_eq!((invoice["order"].clone(), invoice["total"].as_f64()), (json!("4500012345"), Some(4899.0)));
    assert_eq!(invoice["key"].as_str().map(str::len), Some(44));
    assert_eq!(invoice["status"], "received");
    let order = books.run("purchasing.order", &json!({ "order": "4500012345" })).unwrap_or_default();
    assert_eq!((order["status"].clone(), order["total"].as_f64()), (json!("approved"), Some(4899.0)));
    assert_eq!(order["supplier"]["document"], invoice["issuer"]["document"]);
    let lines = order["lines"].as_array().cloned().unwrap_or_default();
    assert!(lines.iter().all(|line| line["received"].is_number() && line["total"].is_number()));
}

#[test]
fn the_seeded_invoices_diverge_from_their_orders_the_way_real_ones_do() {
    let mut books = Books::new();
    let priced = books.run("purchasing.invoice", &json!({ "invoice": "4533" })).unwrap_or_default();
    let sheet = priced["lines"].as_array().and_then(|lines| lines.iter().find(|line| line["code"] == "CHP-3").cloned());
    assert_eq!(sheet.and_then(|line| line["unit_price"].as_f64()), Some(412.5));
    let other = books.run("purchasing.invoice", &json!({ "invoice": "4540" })).unwrap_or_default();
    assert_ne!(other["issuer"]["document"], priced["issuer"]["document"]);
    let pending = books.run("purchasing.invoice", &json!({ "invoice": "4547" })).unwrap_or_default();
    let order = books.run("purchasing.order", &json!({ "order": pending["order"] })).unwrap_or_default();
    assert_eq!(order["status"], "awaiting_approval");
    assert!(books.run("purchasing.invoice", &json!({ "invoice": "9999" })).is_err());
}

#[test]
fn an_invoice_is_released_only_after_it_is_matched_to_its_order() {
    let mut books = Books::new();
    assert!(books.run("purchasing.release", &json!({ "invoice": "4521" })).is_err());
    assert!(books.run("purchasing.match", &json!({ "invoice": "4521", "order": "4500012399" })).is_err());
    let matched =
        books.run("purchasing.match", &json!({ "invoice": "4521", "order": "4500012345" })).unwrap_or_default();
    assert_eq!(matched["status"], "matched");
    let released = books.run("purchasing.release", &json!({ "invoice": "4521" })).unwrap_or_default();
    assert_eq!((released["status"].clone(), released["ref"].clone()), (json!("released"), json!("PAG-4521")));
    assert!(released["due"].as_str().is_some_and(|due| due.len() == 10));
    let read = books.run("purchasing.invoice", &json!({ "invoice": "4521" })).unwrap_or_default();
    assert_eq!(read["status"], "released");
}

#[tokio::test]
async fn the_simulated_purchasing_system_keeps_what_it_recorded_between_calls() {
    crate::memory::install(Arc::new(crate::memory::MemoryValues::default()));
    let provider = loaded("mock_purchasing");
    assert_eq!(provider.descriptor().key.0, "mock-purchasing");
    let http = FakeHttp::new();
    let args = json!({ "invoice": "4521", "order": "4500012345" });
    let matched = provider.call(&http, &creds("compras", json!({})), "purchasing.match", &args, "m-1").await;
    assert!(matched.is_ok(), "{matched:?}");
    let invoice = json!({ "invoice": "4521" });
    let released = provider.call(&http, &creds("compras", json!({})), "purchasing.release", &invoice, "r-1").await;
    assert_eq!(released.map(|value| value["status"].clone()), Ok(json!("released")));
}
