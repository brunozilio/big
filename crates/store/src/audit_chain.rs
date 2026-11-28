use std::fmt::Write as _;

use database::args;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::audit::{AuditDraft, COLUMNS, record_of};
use crate::{Result, Store, signed};

pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

const PAGE: u64 = 1000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AuditVerified {
    pub checked: u64,
    pub first: Option<u64>,
    pub last: Option<u64>,
    pub broken: Option<u64>,
}

pub(crate) fn hash_of(prev: &str, seq: u64, draft: &AuditDraft, detail: &str) -> String {
    let fields = serde_json::json!([
        prev,
        seq,
        draft.at,
        draft.actor,
        draft.subject,
        draft.action,
        draft.policy,
        draft.outcome,
        detail
    ]);
    let digest = Sha256::digest(fields.to_string().as_bytes());
    digest.iter().fold(String::with_capacity(64), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

struct Walk {
    verified: AuditVerified,
    prev: Option<String>,
}

impl Walk {
    fn step(&mut self, record: &crate::AuditRecord, detail: &str) -> bool {
        let draft = AuditDraft {
            at: record.at,
            actor: record.actor.clone(),
            subject: record.subject.clone(),
            action: record.action.clone(),
            policy: record.policy.clone(),
            outcome: record.outcome.clone(),
            detail: serde_json::Value::Null,
        };
        let linked = match &self.prev {
            Some(prev) => *prev == record.prev,
            None => record.seq != 1 || record.prev == GENESIS,
        };
        let sound = linked && hash_of(&record.prev, record.seq, &draft, detail) == record.hash;
        self.verified.checked += 1;
        self.verified.first.get_or_insert(record.seq);
        self.verified.last = Some(record.seq);
        self.prev = Some(record.hash.clone());
        if !sound {
            self.verified.broken = Some(record.seq);
        }
        sound
    }
}

impl Store {
    pub fn verify_audit(&self) -> Result<AuditVerified> {
        let mut walk = Walk { verified: AuditVerified::default(), prev: None };
        let mut after = 0;
        loop {
            let sql = format!("SELECT {COLUMNS} FROM audit_trail WHERE seq > ?1 ORDER BY seq ASC LIMIT ?2");
            let rows = self.read(|conn| Ok(conn.query(&sql, args![signed(after)?, PAGE])?))?;
            if rows.is_empty() {
                return Ok(walk.verified);
            }
            for row in &rows {
                let (record, detail) = record_of(row)?;
                after = record.seq;
                if !walk.step(&record, &detail) {
                    return Ok(walk.verified);
                }
            }
        }
    }
}
