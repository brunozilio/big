use std::time::Duration;

use database::{Conn, Param, Row, args};
use serde::Serialize;

use crate::audit::{AuditFilter, AuditPage, AuditRecord};
use crate::log::now;
use crate::{Result, Store, signed, unsigned};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AuditDelivery {
    pub sink: String,
    pub delivered: u64,
    pub dropped: u64,
    pub attempts: u64,
    pub failure: Option<String>,
    pub at: i64,
}

const COLUMNS: &str = "sink, delivered, dropped, attempts, failure, at";

const OPENED: &str = "INSERT INTO audit_delivery (sink, delivered, dropped, attempts, failure, at)
     VALUES (?1, 0, 0, 0, NULL, ?2) ON CONFLICT (sink) DO NOTHING";

const ADVANCED: &str = "UPDATE audit_delivery SET delivered = ?3, dropped = dropped + ?4, attempts = 0,
     failure = NULL, at = ?5 WHERE sink = ?1 AND delivered = ?2";

const FAILED: &str = "UPDATE audit_delivery SET attempts = attempts + 1, failure = ?2, at = ?3 WHERE sink = ?1";

fn delivery_of(row: &Row) -> Result<AuditDelivery> {
    Ok(AuditDelivery {
        sink: row.get(0)?,
        delivered: unsigned(row.get(1)?)?,
        dropped: unsigned(row.get(2)?)?,
        attempts: unsigned(row.get(3)?)?,
        failure: row.get(4)?,
        at: row.get(5)?,
    })
}

fn found(conn: &mut dyn Conn, sink: &str) -> Result<AuditDelivery> {
    let row = conn.row(&format!("SELECT {COLUMNS} FROM audit_delivery WHERE sink = ?1"), args![sink])?;
    match row {
        Some(row) => delivery_of(&row),
        None => Ok(AuditDelivery { sink: sink.to_owned(), ..AuditDelivery::default() }),
    }
}

fn kept_from(conn: &mut dyn Conn, sinks: &[String]) -> Result<u64> {
    let mut lowest = u64::MAX;
    for sink in sinks {
        lowest = lowest.min(found(conn, sink)?.delivered);
    }
    Ok(lowest)
}

impl Store {
    pub fn audit_delivery(&self, sink: &str) -> Result<AuditDelivery> {
        self.read(|conn| found(conn, sink))
    }

    pub fn audit_deliveries(&self) -> Result<Vec<AuditDelivery>> {
        self.read(|conn| {
            let rows = conn.query(&format!("SELECT {COLUMNS} FROM audit_delivery ORDER BY sink"), &[])?;
            rows.iter().map(delivery_of).collect()
        })
    }

    pub fn audit_outbox(&self, sink: &str, limit: usize) -> Result<(AuditDelivery, Vec<AuditRecord>)> {
        let delivery = self.audit_delivery(sink)?;
        let pending = self.audit(&AuditFilter::default(), AuditPage::After(delivery.delivered), limit)?;
        Ok((delivery, pending))
    }

    pub fn audit_delivered(&self, sink: &str, from: u64, through: u64, dropped: u64) -> Result<bool> {
        let (from, through, dropped) = (signed(from)?, signed(through)?, signed(dropped)?);
        self.write(|conn| {
            let at = now();
            conn.execute(OPENED, args![sink, at])?;
            Ok(conn.execute(ADVANCED, args![sink, from, through, dropped, at])? == 1)
        })
    }

    pub fn audit_undelivered(&self, sink: &str, failure: &str) -> Result<()> {
        self.write(|conn| {
            let at = now();
            conn.execute(OPENED, args![sink, at])?;
            conn.execute(FAILED, args![sink, failure, at])?;
            Ok(())
        })
    }

    pub fn forget_audit_delivered(&self, older_than: Duration, sinks: &[String]) -> Result<usize> {
        let cutoff = now().saturating_sub(i64::try_from(older_than.as_millis()).unwrap_or(i64::MAX));
        self.write(|conn| {
            let kept = signed(kept_from(conn, sinks)?.min(i64::MAX.unsigned_abs()))?;
            let sql = "DELETE FROM audit_trail WHERE at < ?1 AND seq <= ?2";
            let forgotten = conn.execute(sql, &[cutoff.value(), kept.value()])?;
            Ok(usize::try_from(forgotten).unwrap_or(usize::MAX))
        })
    }
}
