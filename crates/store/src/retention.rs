use std::time::Duration;

use database::{Conn, args};

use crate::audit_delivery::{OPENED, found};
use crate::log::now;
use crate::{Result, Store, signed, unsigned};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AuditForgotten {
    pub records: usize,
    pub dropped: u64,
}

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
            let Some(span) = older_than else { return Ok(Forgotten { items: 0, snapshots }) };
            let cutoff = now().saturating_sub(i64::try_from(span.as_millis()).unwrap_or(i64::MAX));
            let items = forget_items(conn, cutoff)?;
            forget_usage(conn, cutoff)?;
            Ok(Forgotten { items, snapshots })
        })
    }

    pub fn forget_audit(&self, older_than: Duration) -> Result<usize> {
        let cutoff = now().saturating_sub(i64::try_from(older_than.as_millis()).unwrap_or(i64::MAX));
        self.write(|conn| Ok(count(conn.execute("DELETE FROM audit_trail WHERE at < ?1", args![cutoff])?)))
    }
}

const OLDEST_PENDING: &str = "SELECT MIN(at) FROM audit_trail WHERE seq > ?1";

const ANCHORED: &str = "seq < (SELECT MAX(seq) FROM audit_trail)";

const DROPPED: &str = "UPDATE audit_delivery SET dropped = dropped + ?2 WHERE sink = ?1";

fn ago(span: Duration) -> i64 {
    now().saturating_sub(i64::try_from(span.as_millis()).unwrap_or(i64::MAX))
}

fn kept_from(conn: &mut dyn Conn, sinks: &[String], max_lag: Option<Duration>) -> Result<(u64, Vec<(String, u64)>)> {
    let mut kept = u64::MAX;
    let mut lagging = Vec::new();
    for sink in sinks {
        let delivered = found(conn, sink)?.delivered;
        let oldest = conn.one::<Option<i64>>(OLDEST_PENDING, args![signed(delivered)?])?.flatten();
        let past = max_lag.zip(oldest).is_some_and(|(lag, oldest)| oldest < ago(lag));
        if past {
            lagging.push((sink.clone(), delivered));
        } else {
            kept = kept.min(delivered);
        }
    }
    Ok((kept, lagging))
}

fn forget_kept(conn: &mut dyn Conn, cutoff: i64, sinks: &[String], max_lag: Option<Duration>) -> Result<AuditForgotten> {
    let (kept, lagging) = kept_from(conn, sinks, max_lag)?;
    let kept = signed(kept.min(i64::MAX.unsigned_abs()))?;
    let mut dropped = 0;
    for (sink, delivered) in lagging {
        let sql = format!("SELECT COUNT(*) FROM audit_trail WHERE at < ?1 AND seq <= ?2 AND seq > ?3 AND {ANCHORED}");
        let lost = conn.one::<i64>(&sql, args![cutoff, kept, signed(delivered)?])?.unwrap_or(0);
        if lost > 0 {
            tracing::warn!(sink, lost, "audit events the sink lagged past max_lag_days on were forgotten");
            conn.execute(OPENED, args![sink, now()])?;
            conn.execute(DROPPED, args![sink, lost])?;
            dropped += unsigned(lost)?;
        }
    }
    let sql = format!("DELETE FROM audit_trail WHERE at < ?1 AND seq <= ?2 AND {ANCHORED}");
    let records = count(conn.execute(&sql, args![cutoff, kept])?);
    Ok(AuditForgotten { records, dropped })
}

impl Store {
    pub fn forget_audit_delivered(
        &self,
        older_than: Duration,
        sinks: &[String],
        max_lag: Option<Duration>,
    ) -> Result<AuditForgotten> {
        let cutoff = ago(older_than);
        self.write(|conn| forget_kept(conn, cutoff, sinks, max_lag))
    }
}

fn month_start() -> i64 {
    let now = jiff::Zoned::now();
    let first = now.date().first_of_month().to_zoned(now.time_zone().clone());
    first.map_or(0, |start| start.timestamp().as_millisecond())
}

fn forget_usage(conn: &mut dyn Conn, cutoff: i64) -> Result<()> {
    conn.execute("DELETE FROM connector_calls WHERE at < ?1", args![cutoff])?;
    conn.execute("DELETE FROM spend WHERE at < ?1", args![cutoff.min(month_start())])?;
    Ok(())
}

fn forget_items(conn: &mut dyn Conn, cutoff: i64) -> Result<usize> {
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
