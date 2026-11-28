use std::fmt::Write as _;

use database::args;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::audit_row::{COLUMNS, Stored};
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

fn fields_of(stored: &Stored) -> serde_json::Value {
    let legacy = serde_json::json!([
        stored.prev,
        stored.seq,
        stored.at,
        stored.actor,
        stored.subject,
        stored.action,
        stored.policy,
        stored.outcome,
        stored.detail
    ]);
    if stored.schema_version == 0 {
        return legacy;
    }
    serde_json::json!([
        legacy,
        stored.schema_version,
        stored.operation,
        stored.friendly_name,
        stored.policy_version,
        stored.decision_code,
        stored.layer,
        stored.failure_stage,
        stored.approval_wait_ms,
        stored.authorized_by,
        stored.creator_type,
        stored.agent
    ])
}

pub(crate) fn hash_of(stored: &Stored) -> String {
    let digest = Sha256::digest(fields_of(stored).to_string().as_bytes());
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
    fn step(&mut self, stored: &Stored) -> bool {
        let linked = match &self.prev {
            Some(prev) => *prev == stored.prev,
            None => stored.seq != 1 || stored.prev == GENESIS,
        };
        let sound = linked && hash_of(stored) == stored.hash;
        self.verified.checked += 1;
        self.verified.first.get_or_insert(stored.seq);
        self.verified.last = Some(stored.seq);
        self.prev = Some(stored.hash.clone());
        if !sound {
            self.verified.broken = Some(stored.seq);
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
                let stored = Stored::read(row)?;
                after = stored.seq;
                if !walk.step(&stored) {
                    return Ok(walk.verified);
                }
            }
        }
    }
}
