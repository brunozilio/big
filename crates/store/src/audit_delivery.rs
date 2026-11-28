use database::{Conn, Row, args};
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

pub(crate) const OPENED: &str = "INSERT INTO audit_delivery (sink, delivered, dropped, attempts, failure, at)
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

pub(crate) fn found(conn: &mut dyn Conn, sink: &str) -> Result<AuditDelivery> {
    let row = conn.row(&format!("SELECT {COLUMNS} FROM audit_delivery WHERE sink = ?1"), args![sink])?;
    match row {
        Some(row) => delivery_of(&row),
        None => Ok(AuditDelivery { sink: sink.to_owned(), ..AuditDelivery::default() }),
    }
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
}
