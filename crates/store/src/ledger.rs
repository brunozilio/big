use database::{Row, Value, args};
use domain::workflow::ItemId;

use crate::{Result, Store, unsigned};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpendRow {
    pub item: String,
    pub step: String,
    pub owner: String,
    pub micros: u64,
    pub unpriced: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SpendOf<'a> {
    pub item: Option<&'a ItemId>,
    pub owner: Option<&'a str>,
}

const SPEND: &str = "SELECT item, step, owner, CAST(COALESCE(SUM(micros), 0) AS BIGINT),
        CAST(COALESCE(SUM(unpriced), 0) AS BIGINT)
     FROM spend WHERE at >= ?1 AND at < ?2";

const GROUPED: &str = " GROUP BY item, step, owner ORDER BY item, step, owner";

fn spend_row(row: &Row) -> database::Result<(String, String, String, i64, i64)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
}

fn spend_query(since: i64, until: i64, of: SpendOf<'_>) -> (String, Vec<Value>) {
    let mut values = args![since, until].to_vec();
    let mut sql = SPEND.to_owned();
    if let Some(item) = of.item {
        values.push(Value::Text(item.as_str().to_owned()));
        sql.push_str(&format!(" AND item = ?{}", values.len()));
    }
    if let Some(owner) = of.owner {
        values.push(Value::Text(owner.to_owned()));
        sql.push_str(&format!(" AND owner = ?{}", values.len()));
    }
    sql.push_str(GROUPED);
    (sql, values)
}

impl Store {
    pub fn touched_items(&self, since: i64, until: i64) -> Result<Vec<(ItemId, i64)>> {
        let rows = self.read(|conn| {
            let rows = conn.query(
                "SELECT item, CAST(MAX(at) AS BIGINT) AS last FROM work_events WHERE at >= ?1 AND at < ?2
                 GROUP BY item ORDER BY last DESC, item",
                args![since, until],
            )?;
            Ok(rows
                .iter()
                .map(|row| Ok((row.get::<String>(0)?, row.get::<i64>(1)?)))
                .collect::<database::Result<Vec<_>>>()?)
        })?;
        Ok(rows.into_iter().map(|(id, last)| (ItemId::from(id), last)).collect())
    }

    pub fn spend_between(&self, since: i64, until: i64, of: SpendOf<'_>) -> Result<Vec<SpendRow>> {
        let (sql, values) = spend_query(since, until, of);
        let rows = self.read(|conn| {
            let rows = conn.query(&sql, &values)?;
            Ok(rows.iter().map(spend_row).collect::<database::Result<Vec<_>>>()?)
        })?;
        rows.into_iter()
            .map(|(item, step, owner, micros, unpriced)| {
                Ok(SpendRow { item, step, owner, micros: unsigned(micros)?, unpriced: unsigned(unpriced)? })
            })
            .collect()
    }
}
