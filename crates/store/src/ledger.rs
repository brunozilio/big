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

const SPEND: &str = "SELECT item, step, owner, CAST(COALESCE(SUM(micros), 0) AS BIGINT),
        CAST(COALESCE(SUM(unpriced), 0) AS BIGINT)
     FROM spend WHERE at >= ?1 AND at < ?2";

const GROUPED: &str = " GROUP BY item, step, owner ORDER BY item, step, owner";

fn spend_row(row: &Row) -> database::Result<(String, String, String, i64, i64)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
}

impl Store {
    pub fn touched_items(&self, since: i64, until: i64) -> Result<Vec<ItemId>> {
        let ids: Vec<String> = self.read(|conn| {
            Ok(conn.all(
                "SELECT DISTINCT item FROM work_events WHERE at >= ?1 AND at < ?2 ORDER BY item",
                args![since, until],
            )?)
        })?;
        Ok(ids.into_iter().map(ItemId::from).collect())
    }

    pub fn spend_between(&self, since: i64, until: i64, item: Option<&ItemId>) -> Result<Vec<SpendRow>> {
        let mut values = args![since, until].to_vec();
        let sql = match item {
            Some(item) => {
                values.push(Value::Text(item.as_str().to_owned()));
                format!("{SPEND} AND item = ?3{GROUPED}")
            }
            None => format!("{SPEND}{GROUPED}"),
        };
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
