use std::collections::HashSet;

use database::args;
use domain::principal::LOCAL_USER;

use crate::{ConnectionRecord, Result, Store};

pub const CONNECTION: &str = "connection";
pub const APPLICATION: &str = "application";
pub const SESSION: &str = "session";
pub const ATTACHMENT: &str = "attachment";

impl Store {
    pub fn set_owner(&self, kind: &str, id: &str, owner: &str) -> Result<()> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO ownership (kind, id, owner) VALUES (?1, ?2, ?3)
                 ON CONFLICT (kind, id) DO UPDATE SET owner = excluded.owner",
                args![kind, id, owner],
            )?;
            Ok(())
        })
    }

    pub fn claim(&self, kind: &str, id: &str, owner: &str) -> Result<String> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO ownership (kind, id, owner) VALUES (?1, ?2, ?3) ON CONFLICT (kind, id) DO NOTHING",
                args![kind, id, owner],
            )?;
            let kept = conn.one("SELECT owner FROM ownership WHERE kind = ?1 AND id = ?2", args![kind, id])?;
            Ok(kept.ok_or_else(|| database::Error::decode("the claim just written is missing"))?)
        })
    }

    pub fn recorded_owner(&self, kind: &str, id: &str) -> Result<Option<String>> {
        self.read(|conn| Ok(conn.one("SELECT owner FROM ownership WHERE kind = ?1 AND id = ?2", args![kind, id])?))
    }

    pub fn owner_of(&self, kind: &str, id: &str) -> Result<String> {
        Ok(self.recorded_owner(kind, id)?.unwrap_or_else(|| LOCAL_USER.to_owned()))
    }

    pub fn owns(&self, kind: &str, id: &str, owner: &str) -> Result<bool> {
        Ok(self.owner_of(kind, id)? == owner)
    }

    pub fn owned(&self, kind: &str) -> Result<Vec<(String, String)>> {
        self.read(|conn| {
            let rows = conn.query("SELECT id, owner FROM ownership WHERE kind = ?1", args![kind])?;
            Ok(rows.iter().map(|row| Ok((row.get(0)?, row.get(1)?))).collect::<database::Result<_>>()?)
        })
    }

    pub fn forget_owner(&self, kind: &str, id: &str) -> Result<()> {
        self.write(|conn| {
            conn.execute("DELETE FROM ownership WHERE kind = ?1 AND id = ?2", args![kind, id])?;
            Ok(())
        })
    }

    pub fn connections_of(&self, owner: &str) -> Result<Vec<ConnectionRecord>> {
        let theirs = self.kept_by(CONNECTION, owner)?;
        Ok(self.connections()?.into_iter().filter(|record| theirs.contains(&record.id)).collect())
    }

    fn kept_by(&self, kind: &str, owner: &str) -> Result<OwnedIds> {
        let all = self.owned(kind)?;
        let others: HashSet<String> = all.iter().filter(|(_, by)| by != owner).map(|(id, _)| id.clone()).collect();
        let mine: HashSet<String> = all.into_iter().filter(|(_, by)| by == owner).map(|(id, _)| id).collect();
        Ok(OwnedIds { mine, others, local: owner == LOCAL_USER })
    }

    pub fn owned_ids(&self, kind: &str, owner: &str) -> Result<OwnedIds> {
        self.kept_by(kind, owner)
    }
}

impl Store {
    pub fn put_git_token(&self, token: &str, owner: &str, connection: &str) -> Result<()> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO git_tokens (token, owner, connection) VALUES (?1, ?2, ?3)
                 ON CONFLICT (token) DO UPDATE SET owner = excluded.owner, connection = excluded.connection",
                args![token, owner, connection],
            )?;
            Ok(())
        })
    }

    pub fn git_token(&self, token: &str) -> Result<Option<(String, String)>> {
        self.read(|conn| {
            let row = conn.row("SELECT owner, connection FROM git_tokens WHERE token = ?1", args![token])?;
            Ok(row.map(|row| Ok::<_, database::Error>((row.get(0)?, row.get(1)?))).transpose()?)
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct OwnedIds {
    mine: HashSet<String>,
    others: HashSet<String>,
    local: bool,
}

impl OwnedIds {
    pub fn contains(&self, id: &str) -> bool {
        self.mine.contains(id) || (self.local && !self.others.contains(id))
    }
}
