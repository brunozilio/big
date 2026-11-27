use domain::principal::{Principal, UserId};
use rusqlite::{OptionalExtension, params};

use crate::{Result, Store};

impl Store {
    pub fn add_user(&self, wanted: &Principal, now: i64) -> Result<bool> {
        let added = self.locked().execute(
            "INSERT INTO users (id, email, name, created, seen) VALUES (?1, ?2, ?3, ?4, ?4) ON CONFLICT DO NOTHING",
            params![wanted.user.as_str(), wanted.email, wanted.name, now],
        )?;
        Ok(added > 0)
    }

    pub fn set_password(&self, user: &UserId, hash: &str, now: i64) -> Result<()> {
        self.locked().execute(
            "INSERT INTO passwords (user, hash, changed) VALUES (?1, ?2, ?3)
             ON CONFLICT (user) DO UPDATE SET hash = excluded.hash, changed = excluded.changed",
            params![user.as_str(), hash, now],
        )?;
        Ok(())
    }

    pub fn password_of(&self, user: &UserId) -> Result<Option<String>> {
        Ok(self
            .locked()
            .query_row("SELECT hash FROM passwords WHERE user = ?1", [user.as_str()], |row| row.get(0))
            .optional()?)
    }

    pub fn login_locked(&self, login: &str, now: i64) -> Result<bool> {
        let until: Option<i64> = self
            .locked()
            .query_row("SELECT locked_until FROM login_failures WHERE login = ?1", [login], |row| row.get(0))
            .optional()?;
        Ok(until.is_some_and(|until| until > now))
    }

    pub fn login_failed(&self, login: &str, now: i64, most: i64, lock_seconds: i64) -> Result<()> {
        self.write(|transaction| {
            let failures: i64 = transaction
                .query_row("SELECT failures FROM login_failures WHERE login = ?1", [login], |row| row.get(0))
                .optional()?
                .unwrap_or(0)
                + 1;
            let (failures, until) = if failures >= most { (0, now + lock_seconds) } else { (failures, 0) };
            transaction.execute(
                "INSERT INTO login_failures (login, failures, locked_until) VALUES (?1, ?2, ?3)
                 ON CONFLICT (login) DO UPDATE SET failures = excluded.failures, locked_until = excluded.locked_until",
                params![login, failures, until],
            )?;
            Ok(())
        })
    }

    pub fn login_succeeded(&self, login: &str) -> Result<()> {
        self.locked().execute("DELETE FROM login_failures WHERE login = ?1", [login])?;
        Ok(())
    }
}
