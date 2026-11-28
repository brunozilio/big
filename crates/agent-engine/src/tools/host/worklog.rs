use std::time::{SystemTime, UNIX_EPOCH};

use domain::principal::LOCAL_USER;
use domain::workflow::ledger::{Ask, DAY_MS};
use serde_json::Value;

use super::Reach;

pub const SPACE: &str = "worklog";

const LISTED: usize = 20;
const LISTED_AT_MOST: usize = 100;

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    let ledger = reach.workspace.services.ledger.as_ref().ok_or("this run has no work log to look into")?;
    let ledger = &ledger.0;
    let asker = reach.workspace.owner.as_deref().unwrap_or(LOCAL_USER);
    let ask = ask(arguments);
    match name {
        "items" => ledger.items(asker, &ask).map(|views| shown(&views)),
        "item" => {
            let id = arguments.get("item").and_then(Value::as_str).filter(|id| !id.trim().is_empty());
            let id = id.ok_or("item must be a work item id")?;
            ledger.item(asker, id.trim(), &ask).map(|view| shown(&view))
        }
        "cost" => ledger.cost(asker, &ask).map(|view| shown(&view)),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
}

fn moment(arguments: &Value, key: &str) -> Option<i64> {
    arguments.get(key).and_then(Value::as_i64).filter(|at| *at >= 0)
}

fn ask(arguments: &Value) -> Ask {
    let until_ms = moment(arguments, "until_ms").unwrap_or_else(now_ms);
    let since_ms = moment(arguments, "since_ms").unwrap_or(until_ms.saturating_sub(DAY_MS)).min(until_ms);
    let person = arguments.get("person").and_then(Value::as_str).map(str::trim).filter(|person| !person.is_empty());
    let limit = arguments.get("limit").and_then(Value::as_u64).and_then(|limit| usize::try_from(limit).ok());
    Ask {
        person: person.map(str::to_owned),
        since_ms,
        until_ms,
        limit: limit.unwrap_or(LISTED).clamp(1, LISTED_AT_MOST),
    }
}

fn shown(value: &impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use domain::workflow::ledger::{CostView, HostLedger, ItemView, Ledger, cost_view};
    use serde_json::json;

    use super::super::Fence;
    use super::*;
    use crate::tools::Workspace;

    #[derive(Default)]
    struct Kept(Mutex<Vec<(String, Ask)>>);

    impl Ledger for Kept {
        fn items(&self, asker: &str, ask: &Ask) -> Result<Vec<ItemView>, String> {
            self.0.lock().map_err(|_| "poisoned")?.push((asker.to_owned(), ask.clone()));
            if ask.person.as_deref() == Some("u-bia") { Err("not yours".into()) } else { Ok(Vec::new()) }
        }
        fn item(&self, _asker: &str, id: &str, _ask: &Ask) -> Result<ItemView, String> {
            Err(format!("no item {id}"))
        }
        fn cost(&self, _asker: &str, ask: &Ask) -> Result<CostView, String> {
            Ok(cost_view(ask.person.clone(), ask.window(), &[], &|id| id.to_owned()))
        }
    }

    fn asked(workspace: &Workspace, name: &str, arguments: Value) -> Result<Value, String> {
        call(&Reach { workspace, fence: Fence::of(&[]), streams: None }, name, &arguments)
    }

    #[test]
    fn the_work_log_asks_the_host_as_the_run_s_owner_within_a_window() {
        let mut workspace = Workspace::new(std::env::temp_dir());
        assert!(asked(&workspace, "items", Value::Null).is_err(), "no work log without a host that keeps one");
        let kept = Arc::new(Kept::default());
        workspace.services.ledger = Some(HostLedger(Arc::clone(&kept) as Arc<dyn Ledger>));
        workspace.owner = Some("u-ana".into());
        assert_eq!(asked(&workspace, "items", json!({"since_ms": 10, "until_ms": 20, "limit": 500})), Ok(json!([])));
        assert_eq!(asked(&workspace, "items", json!({"person": "u-bia"})), Err("not yours".into()));
        let seen = kept.0.lock().expect("kept").clone();
        assert_eq!(seen[0], ("u-ana".into(), Ask { person: None, since_ms: 10, until_ms: 20, limit: 100 }));
        assert_eq!(seen[1].1.until_ms - seen[1].1.since_ms, DAY_MS, "the last day by default");
        let cost = asked(&workspace, "cost", json!({"since_ms": 5, "until_ms": 9})).expect("cost");
        assert_eq!(cost["since_ms"], 5);
        assert!(asked(&workspace, "item", json!({})).is_err());
        assert_eq!(asked(&workspace, "item", json!({"item": "i9"})), Err("no item i9".into()));
        assert!(asked(&workspace, "forget", Value::Null).is_err());
    }
}
