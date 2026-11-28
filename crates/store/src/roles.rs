use database::args;
use domain::principal::UserId;

use crate::{Result, Store};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldRole {
    pub user: UserId,
    pub source: String,
    pub role: String,
}

fn held(row: &database::Row) -> database::Result<HeldRole> {
    Ok(HeldRole { user: UserId(row.get(0)?), source: row.get(1)?, role: row.get(2)? })
}

impl Store {
    pub fn roles_of(&self, user: &UserId) -> Result<Vec<HeldRole>> {
        self.read(|conn| {
            let rows = conn.query(
                "SELECT \"user\", source, role FROM user_roles WHERE \"user\" = ?1 ORDER BY source, role",
                args![user.as_str()],
            )?;
            Ok(rows.iter().map(held).collect::<database::Result<_>>()?)
        })
    }

    pub fn all_roles(&self) -> Result<Vec<HeldRole>> {
        self.read(|conn| {
            let rows =
                conn.query("SELECT \"user\", source, role FROM user_roles ORDER BY \"user\", source, role", &[])?;
            Ok(rows.iter().map(held).collect::<database::Result<_>>()?)
        })
    }

    pub fn set_roles(&self, user: &UserId, source: &str, roles: &[String]) -> Result<()> {
        self.write(|conn| {
            conn.execute("DELETE FROM user_roles WHERE \"user\" = ?1 AND source = ?2", args![user.as_str(), source])?;
            for role in roles {
                conn.execute(
                    "INSERT INTO user_roles (\"user\", source, role) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
                    args![user.as_str(), source, role],
                )?;
            }
            Ok(())
        })
    }
}
