use database::args;

use crate::{Result, Store};

impl Store {
    pub fn agent_number(&self, owner: &str) -> Result<i64> {
        self.write(|conn| {
            if let Some(found) = conn.one("SELECT number FROM agent_people WHERE owner = ?1", args![owner])? {
                return Ok(found);
            }
            let next: i64 = conn.one("SELECT COALESCE(MAX(number), 0) + 1 FROM agent_people", &[])?.unwrap_or(1);
            conn.execute(
                "INSERT INTO agent_people (owner, number) VALUES (?1, ?2) ON CONFLICT (owner) DO NOTHING",
                args![owner, next],
            )?;
            Ok(conn.one("SELECT number FROM agent_people WHERE owner = ?1", args![owner])?.unwrap_or(next))
        })
    }
}
