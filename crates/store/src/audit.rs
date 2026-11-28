use database::{Conn, Param, Row, Value};
use domain::audit::AuditEvent;
use serde::Serialize;

use crate::audit_chain::{GENESIS, hash_of};
use crate::audit_row::{COLUMNS, Stored};
use crate::{Result, Store, signed, unsigned};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuditRecord {
    pub seq: u64,
    #[serde(flatten)]
    pub event: AuditEvent,
    pub prev: String,
    pub hash: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditFilter {
    pub actor: Option<String>,
    pub subject: Option<String>,
    pub action: Option<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditPage {
    Before(Option<u64>),
    After(u64),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AuditFacets {
    pub actors: Vec<String>,
    pub subjects: Vec<String>,
    pub actions: Vec<String>,
}

const LAST: &str = "SELECT seq, hash FROM audit_trail ORDER BY seq DESC LIMIT 1";

fn insert() -> String {
    let marks: Vec<String> = (1..=COLUMNS.split(',').count()).map(|at| format!("?{at}")).collect();
    format!("INSERT INTO audit_trail ({COLUMNS}) VALUES ({})", marks.join(", "))
}

fn record_of(row: &Row) -> Result<AuditRecord> {
    let stored = Stored::read(row)?;
    Ok(AuditRecord { seq: stored.seq, event: stored.event()?, prev: stored.prev, hash: stored.hash })
}

fn appended(conn: &mut dyn Conn, events: &[AuditEvent]) -> Result<u64> {
    let last = conn.row(LAST, &[])?;
    let (mut seq, mut prev) = match last {
        Some(row) => (unsigned(row.get(0)?)?, row.get::<String>(1)?),
        None => (0, GENESIS.to_owned()),
    };
    let sql = insert();
    for event in events {
        seq += 1;
        signed(seq)?;
        let mut stored = Stored::of_event(seq, event);
        stored.prev = prev;
        stored.hash = hash_of(&stored);
        conn.execute(&sql, &stored.values())?;
        prev = stored.hash;
    }
    Ok(seq)
}

fn narrowed(filter: &AuditFilter, page: AuditPage, limit: usize) -> Result<(String, Vec<Value>)> {
    let mut clauses = Vec::new();
    let mut values: Vec<Value> = Vec::new();
    let mut add = |clause: &str, value: Value| {
        values.push(value);
        clauses.push(format!("{clause} ?{}", values.len()));
    };
    let texts = [("actor =", &filter.actor), ("subject =", &filter.subject), ("action =", &filter.action)];
    for (clause, wanted) in texts {
        if let Some(wanted) = wanted.as_deref().filter(|text| !text.is_empty()) {
            add(clause, wanted.value());
        }
    }
    for (clause, bound) in [("at >=", filter.from), ("at <", filter.to)] {
        if let Some(bound) = bound {
            add(clause, bound.value());
        }
    }
    let order = match page {
        AuditPage::Before(Some(seq)) => {
            add("seq <", signed(seq)?.value());
            "DESC"
        }
        AuditPage::Before(None) => "DESC",
        AuditPage::After(seq) => {
            add("seq >", signed(seq)?.value());
            "ASC"
        }
    };
    let filtered = if clauses.is_empty() { String::new() } else { format!(" WHERE {}", clauses.join(" AND ")) };
    values.push(limit.value());
    let sql = format!("SELECT {COLUMNS} FROM audit_trail{filtered} ORDER BY seq {order} LIMIT ?{}", values.len());
    Ok((sql, values))
}

fn distinct(conn: &mut dyn Conn, column: &str) -> Result<Vec<String>> {
    let sql = format!("SELECT DISTINCT {column} FROM audit_trail WHERE {column} IS NOT NULL ORDER BY {column}");
    Ok(conn.all(&sql, &[])?)
}

impl Store {
    pub fn append_audit(&self, events: &[AuditEvent]) -> Result<u64> {
        self.write(|conn| appended(conn, events))
    }

    pub fn audit(&self, filter: &AuditFilter, page: AuditPage, limit: usize) -> Result<Vec<AuditRecord>> {
        let (sql, values) = narrowed(filter, page, limit)?;
        self.read(|conn| {
            conn.query(&sql, &values)?.iter().map(record_of).collect()
        })
    }

    pub fn audit_facets(&self) -> Result<AuditFacets> {
        self.read(|conn| {
            Ok(AuditFacets {
                actors: distinct(conn, "actor")?,
                subjects: distinct(conn, "subject")?,
                actions: distinct(conn, "action")?,
            })
        })
    }
}
