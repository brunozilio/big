use database::{Conn, Param, Row, Value, args};
use serde::Serialize;

use crate::audit_chain::{GENESIS, hash_of};
use crate::{Result, Store, signed, unsigned};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AuditDraft {
    pub at: i64,
    pub actor: Option<String>,
    pub subject: Option<String>,
    pub action: String,
    pub policy: Option<String>,
    pub outcome: Option<String>,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuditRecord {
    pub seq: u64,
    pub at: i64,
    pub actor: Option<String>,
    pub subject: Option<String>,
    pub action: String,
    pub policy: Option<String>,
    pub outcome: Option<String>,
    pub detail: serde_json::Value,
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

pub(crate) const COLUMNS: &str = "seq, at, actor, subject, action, policy, outcome, detail, prev, hash";

const LAST: &str = "SELECT seq, hash FROM audit_trail ORDER BY seq DESC LIMIT 1";

const INSERT: &str = "INSERT INTO audit_trail (seq, at, actor, subject, action, policy, outcome, detail, prev, hash)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";

pub(crate) fn record_of(row: &Row) -> Result<(AuditRecord, String)> {
    let detail: String = row.get(7)?;
    let record = AuditRecord {
        seq: unsigned(row.get(0)?)?,
        at: row.get(1)?,
        actor: row.get(2)?,
        subject: row.get(3)?,
        action: row.get(4)?,
        policy: row.get(5)?,
        outcome: row.get(6)?,
        detail: serde_json::from_str(&detail)?,
        prev: row.get(8)?,
        hash: row.get(9)?,
    };
    Ok((record, detail))
}

fn appended(conn: &mut dyn Conn, drafts: &[AuditDraft]) -> Result<u64> {
    let last = conn.row(LAST, &[])?;
    let (mut seq, mut prev) = match last {
        Some(row) => (unsigned(row.get(0)?)?, row.get::<String>(1)?),
        None => (0, GENESIS.to_owned()),
    };
    for draft in drafts {
        seq += 1;
        let detail = draft.detail.to_string();
        let hash = hash_of(&prev, seq, draft, &detail);
        conn.execute(
            INSERT,
            args![
                signed(seq)?,
                draft.at,
                draft.actor,
                draft.subject,
                draft.action,
                draft.policy,
                draft.outcome,
                detail,
                prev,
                hash
            ],
        )?;
        prev = hash;
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
    pub fn append_audit(&self, drafts: &[AuditDraft]) -> Result<u64> {
        self.write(|conn| appended(conn, drafts))
    }

    pub fn audit(&self, filter: &AuditFilter, page: AuditPage, limit: usize) -> Result<Vec<AuditRecord>> {
        let (sql, values) = narrowed(filter, page, limit)?;
        self.read(|conn| {
            conn.query(&sql, &values)?.iter().map(|row| record_of(row).map(|(record, _)| record)).collect()
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
