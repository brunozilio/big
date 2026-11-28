use std::collections::BTreeMap;

use serde_json::{Value, json};
use store::{ConnectionRecord, ConnectorCalls};

#[derive(Debug, Default)]
struct PerConnection {
    calls: u64,
    failed: u64,
    by_origin: BTreeMap<String, u64>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Unowned {
    pub calls: u64,
}

pub fn by_connection(
    rows: &[ConnectorCalls],
    records: &[ConnectionRecord],
    mine: impl Fn(&str) -> bool,
) -> (Vec<Value>, Unowned) {
    let mut grouped: BTreeMap<&str, PerConnection> = BTreeMap::new();
    let mut unowned = Unowned::default();
    for row in rows.iter().filter(|row| mine(&row.connection)) {
        let kept = grouped.entry(row.connection.as_str()).or_default();
        kept.calls += row.calls;
        kept.failed += row.failed;
        if let Some(origin) = &row.origin {
            *kept.by_origin.entry(origin.clone()).or_default() += row.calls;
        }
        if !row.attributed {
            unowned.calls += row.calls;
        }
    }
    let mut listed: Vec<(&str, PerConnection)> = grouped.into_iter().collect();
    listed.sort_by(|a, b| b.1.calls.cmp(&a.1.calls).then(a.0.cmp(b.0)));
    let found = listed
        .into_iter()
        .map(|(id, counted)| {
            let record = records.iter().find(|record| record.id == id);
            json!({
                "id": id,
                "name": record.map_or(id, |record| record.label.as_str()),
                "provider": record.map(|record| record.provider.as_str()),
                "calls": counted.calls,
                "failed": counted.failed,
                "byOrigin": counted.by_origin,
            })
        })
        .collect();
    (found, unowned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(connection: &str, origin: Option<&str>, attributed: bool, calls: u64, failed: u64) -> ConnectorCalls {
        ConnectorCalls { connection: connection.into(), origin: origin.map(str::to_owned), attributed, calls, failed }
    }

    fn record(id: &str, label: &str) -> ConnectionRecord {
        ConnectionRecord {
            id: id.into(),
            provider: "tracker-x".into(),
            label: label.into(),
            account: None,
            capabilities: Vec::new(),
            settings: Value::Null,
        }
    }

    #[test]
    fn calls_are_listed_per_connection_by_origin_and_the_ones_no_item_owns_are_counted_apart() {
        let rows = [
            row("c1", Some("step"), true, 4, 1),
            row("c1", None, false, 2, 0),
            row("c2", Some("trigger"), false, 9, 0),
            row("c3", Some("step"), true, 50, 0),
        ];
        let (listed, unowned) = by_connection(&rows, &[record("c1", "Acme")], |id| id != "c3");
        assert_eq!(listed[0]["id"], "c2");
        assert_eq!(listed[0]["name"], "c2", "a connection gone from the store keeps its id");
        assert_eq!(listed[1]["name"], "Acme");
        assert_eq!(listed[1]["provider"], "tracker-x");
        assert_eq!((listed[1]["calls"].as_u64(), listed[1]["failed"].as_u64()), (Some(6), Some(1)));
        assert_eq!(listed[1]["byOrigin"], json!({ "step": 4 }));
        assert_eq!(listed.len(), 2, "a connection the person cannot see is left out");
        assert_eq!(unowned, Unowned { calls: 11 });
    }
}
