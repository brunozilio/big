use std::time::Duration;

use rusqlite::{Transaction, params};

use crate::log::now;
use crate::{Result, Store};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Forgotten {
    pub items: usize,
    pub snapshots: usize,
}

const OLD_SNAPSHOTS: &str = "DELETE FROM snapshots WHERE item IN (SELECT id FROM items WHERE ended = 1)
     AND seq < (SELECT MAX(seq) FROM snapshots kept WHERE kept.item = snapshots.item)";

const LONG_ENDED: &str = "SELECT id FROM items i WHERE i.ended = 1
     AND (SELECT MAX(at) FROM work_events e WHERE e.item = i.id) < ?1
     AND NOT EXISTS (SELECT 1 FROM items a WHERE a.ended = 0 AND instr(a.state, '\"' || i.id || '\"') > 0)";

impl Store {
    pub fn forget_ended(&self, older_than: Option<Duration>) -> Result<Forgotten> {
        self.write(|transaction| {
            let snapshots = transaction.execute(OLD_SNAPSHOTS, [])?;
            let items = match older_than {
                Some(span) => forget_items(transaction, span)?,
                None => 0,
            };
            Ok(Forgotten { items, snapshots })
        })
    }
}

fn forget_items(transaction: &Transaction<'_>, span: Duration) -> Result<usize> {
    let cutoff = now().saturating_sub(i64::try_from(span.as_millis()).unwrap_or(i64::MAX));
    let ids: Vec<String> = {
        let mut statement = transaction.prepare(LONG_ENDED)?;
        statement.query_map([cutoff], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?
    };
    for id in &ids {
        for table in ["DELETE FROM work_events WHERE item = ?1", "DELETE FROM snapshots WHERE item = ?1"] {
            transaction.execute(table, params![id])?;
        }
        transaction.execute("DELETE FROM archived_items WHERE item = ?1", params![id])?;
        transaction.execute("DELETE FROM items WHERE id = ?1", params![id])?;
    }
    Ok(ids.len())
}
