use database::args;

use crate::{Result, Store};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorCall<'a> {
    pub at: i64,
    pub connection: &'a str,
    pub action: &'a str,
    pub origin: Option<&'a str>,
    pub item: Option<&'a str>,
    pub step: Option<&'a str>,
    pub ok: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConnectorCalls {
    pub connection: String,
    pub origin: Option<String>,
    pub attributed: bool,
    pub calls: u64,
    pub failed: u64,
}

const TALLY: &str = "SELECT connection, origin, CASE WHEN item IS NULL THEN 0 ELSE 1 END AS attributed,
        COUNT(*), CAST(SUM(CASE WHEN ok = 0 THEN 1 ELSE 0 END) AS BIGINT)
     FROM connector_calls WHERE at >= ?1
     GROUP BY connection, origin, attributed ORDER BY connection, COALESCE(origin, ''), attributed";

impl Store {
    pub fn count_connector_call(&self, call: &ConnectorCall<'_>) -> Result<()> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO connector_calls (at, connection, action, origin, item, step, ok)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                args![call.at, call.connection, call.action, call.origin, call.item, call.step, call.ok],
            )?;
            Ok(())
        })
    }

    pub fn connector_calls_since(&self, at: i64) -> Result<Vec<ConnectorCalls>> {
        self.read(|conn| {
            let rows = conn.query(TALLY, args![at])?;
            let counted = rows.iter().map(|row| {
                Ok(ConnectorCalls {
                    connection: row.get(0)?,
                    origin: row.get(1)?,
                    attributed: row.get(2)?,
                    calls: row.get::<i64>(3)?.unsigned_abs(),
                    failed: row.get::<i64>(4)?.unsigned_abs(),
                })
            });
            Ok(counted.collect::<database::Result<Vec<_>>>()?)
        })
    }
}
