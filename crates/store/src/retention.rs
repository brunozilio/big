use std::time::Duration;

use database::{Conn, args};

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
     AND NOT EXISTS (SELECT 1 FROM items a WHERE a.ended = 0 AND REPLACE(a.state, '\"' || i.id || '\"', '') <> a.state)";

fn count(changed: u64) -> usize {
    usize::try_from(changed).unwrap_or(usize::MAX)
}

impl Store {
    pub fn forget_ended(&self, older_than: Option<Duration>) -> Result<Forgotten> {
        self.write(|conn| {
            let snapshots = count(conn.execute(OLD_SNAPSHOTS, &[])?);
            let items = match older_than {
                Some(span) => forget_items(conn, span)?,
                None => 0,
            };
            Ok(Forgotten { items, snapshots })
        })
    }
}

fn forget_items(conn: &mut dyn Conn, span: Duration) -> Result<usize> {
    let cutoff = now().saturating_sub(i64::try_from(span.as_millis()).unwrap_or(i64::MAX));
    let ids: Vec<String> = conn.all(LONG_ENDED, args![cutoff])?;
    for id in &ids {
        for table in ["DELETE FROM work_events WHERE item = ?1", "DELETE FROM snapshots WHERE item = ?1"] {
            conn.execute(table, args![id])?;
        }
        conn.execute("DELETE FROM archived_items WHERE item = ?1", args![id])?;
        conn.execute("DELETE FROM items WHERE id = ?1", args![id])?;
    }
    Ok(ids.len())
}
