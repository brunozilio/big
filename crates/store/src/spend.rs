use database::args;
use domain::billing::{Basis, Cost};
use domain::quota::Scope;

use crate::{Result, Store};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spent {
    pub at: i64,
    pub item: String,
    pub step: String,
    pub agent: String,
    pub application: String,
    pub owner: String,
    pub cost: Cost,
}

const fn column(scope: Scope) -> Option<&'static str> {
    match scope {
        Scope::Agent => Some("agent"),
        Scope::Application => Some("application"),
        Scope::Person => Some("owner"),
        Scope::Run | Scope::Session | Scope::Item | Scope::Account | Scope::Workspace => None,
    }
}

impl Store {
    pub fn record_spend(&self, spent: &Spent) -> Result<()> {
        if spent.cost == Cost::default() {
            return Ok(());
        }
        self.write(|conn| {
            conn.execute(
                "INSERT INTO spend (at, item, step, agent, application, owner, micros, unpriced)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                args![
                    spent.at,
                    spent.item,
                    spent.step,
                    spent.agent,
                    spent.application,
                    spent.owner,
                    spent.cost.known_micros,
                    spent.cost.unpriced_calls
                ],
            )?;
            Ok(())
        })
    }

    pub fn spent_since(&self, at: i64, scope: Scope, id: &str) -> Result<Cost> {
        let Some(column) = column(scope) else { return Ok(Cost::default()) };
        if id.is_empty() {
            return Ok(Cost::default());
        }
        let sql = format!(
            "SELECT CAST(COALESCE(SUM(micros), 0) AS BIGINT), CAST(COALESCE(SUM(unpriced), 0) AS BIGINT)
             FROM spend WHERE at >= ?1 AND {column} = ?2"
        );
        self.read(|conn| {
            let row = conn.row(&sql, args![at, id])?;
            let (known, unpriced): (i64, i64) = match row {
                Some(row) => (row.get(0)?, row.get(1)?),
                None => (0, 0),
            };
            Ok(Cost {
                known_micros: known.unsigned_abs(),
                unpriced_calls: unpriced.unsigned_abs(),
                basis: Basis::Unknown,
            })
        })
    }
}
