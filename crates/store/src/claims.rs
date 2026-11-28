use database::{Row, args};
use serde::Serialize;

use crate::{Result, Store};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Claim {
    pub target: String,
    pub item: String,
    pub person: String,
    pub name: String,
    pub at: i64,
}

const COLUMNS: &str = "target, item, person, name, at";

fn claim(row: &Row) -> database::Result<Claim> {
    Ok(Claim { target: row.get(0)?, item: row.get(1)?, person: row.get(2)?, name: row.get(3)?, at: row.get(4)? })
}

impl Store {
    pub fn claim_inbox(&self, wanted: &Claim) -> Result<Claim> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO inbox_claims (target, item, person, name, at) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT DO NOTHING",
                args![wanted.target, wanted.item, wanted.person, wanted.name, wanted.at],
            )?;
            let row =
                conn.row(&format!("SELECT {COLUMNS} FROM inbox_claims WHERE target = ?1"), args![wanted.target])?;
            Ok(row.as_ref().map(claim).transpose()?.unwrap_or_else(|| wanted.clone()))
        })
    }

    pub fn inbox_claims(&self) -> Result<Vec<Claim>> {
        self.read(|conn| {
            let rows = conn.query(&format!("SELECT {COLUMNS} FROM inbox_claims ORDER BY at"), &[])?;
            Ok(rows.iter().map(claim).collect::<database::Result<_>>()?)
        })
    }

    pub fn release_inbox(&self, target: &str, person: Option<&str>) -> Result<bool> {
        let removed = self.write(|conn| match person {
            Some(person) => {
                Ok(conn.execute("DELETE FROM inbox_claims WHERE target = ?1 AND person = ?2", args![target, person])?)
            }
            None => Ok(conn.execute("DELETE FROM inbox_claims WHERE target = ?1", args![target])?),
        })?;
        Ok(removed > 0)
    }

    pub fn forget_item_claims(&self, item: &str) -> Result<u64> {
        self.write(|conn| Ok(conn.execute("DELETE FROM inbox_claims WHERE item = ?1", args![item])?))
    }
}
