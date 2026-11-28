use crate::{Database, Result, args};

const TABLE: &str = "CREATE TABLE IF NOT EXISTS leases (
    name TEXT PRIMARY KEY,
    holder TEXT NOT NULL,
    until BIGINT NOT NULL,
    wanted_by TEXT NOT NULL DEFAULT '',
    wanted_until BIGINT NOT NULL DEFAULT 0
)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turn {
    pub held: bool,
    pub wanted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub name: String,
    pub holder: String,
    pub seconds: i64,
}

impl Lease {
    pub fn new(name: impl Into<String>, holder: impl Into<String>, seconds: i64) -> Self {
        Self { name: name.into(), holder: holder.into(), seconds }
    }

    pub fn take(&self, db: &Database, now: i64) -> Result<Turn> {
        let lease = self.clone();
        db.write(move |conn| {
            conn.batch(TABLE)?;
            let until = now + lease.seconds;
            let took = conn.execute(
                "INSERT INTO leases (name, holder, until) VALUES (?1, ?2, ?3)
                 ON CONFLICT (name) DO UPDATE SET holder = excluded.holder, until = excluded.until,
                 wanted_by = CASE WHEN leases.holder = excluded.holder THEN leases.wanted_by ELSE '' END,
                 wanted_until = CASE WHEN leases.holder = excluded.holder THEN leases.wanted_until ELSE 0 END
                 WHERE leases.holder = excluded.holder OR leases.until < ?4",
                args![lease.name, lease.holder, until, now],
            )?;
            if took == 0 {
                conn.execute(
                    "UPDATE leases SET wanted_by = ?2, wanted_until = ?3 WHERE name = ?1 AND holder <> ?2",
                    args![lease.name, lease.holder, until],
                )?;
                return Ok(Turn { held: false, wanted: false });
            }
            let wanted = conn
                .one::<i64>(
                    "SELECT COUNT(*) FROM leases WHERE name = ?1 AND wanted_by <> '' AND wanted_by <> ?2 AND wanted_until >= ?3",
                    args![lease.name, lease.holder, now],
                )?
                .unwrap_or(0)
                > 0;
            Ok(Turn { held: true, wanted })
        })
    }

    pub fn release(&self, db: &Database) -> Result<()> {
        let lease = self.clone();
        db.write(move |conn| {
            conn.batch(TABLE)?;
            conn.execute("DELETE FROM leases WHERE name = ?1 AND holder = ?2", args![lease.name, lease.holder])?;
            Ok(())
        })
    }
}
