use domain::principal::{Principal, UserId};
use rusqlite::{OptionalExtension, params};

use crate::{Result, Store};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passkey {
    pub id: String,
    pub user: UserId,
    pub public_key: Vec<u8>,
    pub algorithm: i64,
    pub counter: u32,
    pub name: String,
    pub created: i64,
    pub used: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub principal: Principal,
    pub created: i64,
    pub seen: i64,
    pub sessions: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeCheck {
    Matched,
    Wrong,
    Gone,
}

fn principal(row: &rusqlite::Row<'_>) -> rusqlite::Result<Principal> {
    Ok(Principal { user: UserId(row.get(0)?), email: row.get(1)?, name: row.get(2)? })
}

fn passkey(row: &rusqlite::Row<'_>) -> rusqlite::Result<Passkey> {
    Ok(Passkey {
        id: row.get(0)?,
        user: UserId(row.get(1)?),
        public_key: row.get(2)?,
        algorithm: row.get(3)?,
        counter: u32::try_from(row.get::<_, i64>(4)?).unwrap_or(u32::MAX),
        name: row.get(5)?,
        created: row.get(6)?,
        used: row.get(7)?,
    })
}

const PASSKEY_COLUMNS: &str = "id, user, public_key, algorithm, counter, name, created, used";

impl Store {
    pub fn user_by_email(&self, email: &str) -> Result<Option<Principal>> {
        Ok(self
            .locked()
            .query_row("SELECT id, email, name FROM users WHERE email = ?1", [email], principal)
            .optional()?)
    }

    pub fn user_ids(&self) -> Result<Vec<String>> {
        let connection = self.locked();
        let mut statement = connection.prepare("SELECT id FROM users")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn user(&self, id: &UserId) -> Result<Option<Principal>> {
        Ok(self
            .locked()
            .query_row("SELECT id, email, name FROM users WHERE id = ?1", [id.as_str()], principal)
            .optional()?)
    }

    pub fn ensure_user(&self, wanted: &Principal, now: i64) -> Result<Principal> {
        self.write(|transaction| {
            transaction.execute(
                "INSERT INTO users (id, email, name, created, seen) VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT (email) DO UPDATE SET seen = excluded.seen,
                   name = CASE WHEN excluded.name = '' THEN users.name ELSE excluded.name END",
                params![wanted.user.as_str(), wanted.email, wanted.name, now],
            )?;
            Ok(transaction.query_row(
                "SELECT id, email, name FROM users WHERE email = ?1",
                [&wanted.email],
                principal,
            )?)
        })
    }

    pub fn open_session(&self, token: &str, user: &UserId, now: i64, expires: i64) -> Result<()> {
        self.locked().execute(
            "INSERT INTO auth_sessions (token, user, created, expires) VALUES (?1, ?2, ?3, ?4)",
            params![token, user.as_str(), now, expires],
        )?;
        Ok(())
    }

    pub fn session_user(&self, token: &str, now: i64) -> Result<Option<Principal>> {
        Ok(self
            .locked()
            .query_row(
                "SELECT u.id, u.email, u.name FROM auth_sessions s JOIN users u ON u.id = s.user
                 WHERE s.token = ?1 AND s.expires > ?2",
                params![token, now],
                principal,
            )
            .optional()?)
    }

    pub fn people(&self, now: i64) -> Result<Vec<Person>> {
        let connection = self.locked();
        let mut statement = connection.prepare(
            "SELECT u.id, u.email, u.name, u.created, u.seen,
                    (SELECT COUNT(*) FROM auth_sessions s WHERE s.user = u.id AND s.expires > ?1)
             FROM users u ORDER BY u.email",
        )?;
        let rows = statement.query_map([now], |row| {
            Ok(Person {
                principal: principal(row)?,
                created: row.get(3)?,
                seen: row.get(4)?,
                sessions: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn close_sessions_of(&self, user: &UserId) -> Result<usize> {
        Ok(self.locked().execute("DELETE FROM auth_sessions WHERE user = ?1", [user.as_str()])?)
    }

    pub fn close_session(&self, token: &str) -> Result<()> {
        self.locked().execute("DELETE FROM auth_sessions WHERE token = ?1", [token])?;
        Ok(())
    }

    pub fn forget_expired(&self, now: i64) -> Result<()> {
        self.write(|transaction| {
            transaction.execute("DELETE FROM auth_sessions WHERE expires <= ?1", [now])?;
            transaction.execute("DELETE FROM auth_codes WHERE expires <= ?1", [now])?;
            transaction.execute("DELETE FROM auth_challenges WHERE expires <= ?1", [now])?;
            Ok(())
        })
    }

    pub fn put_code(&self, email: &str, code: &str, expires: i64) -> Result<()> {
        self.locked().execute(
            "INSERT INTO auth_codes (email, code, expires, attempts) VALUES (?1, ?2, ?3, 0)
             ON CONFLICT (email) DO UPDATE SET code = excluded.code, expires = excluded.expires, attempts = 0",
            params![email, code, expires],
        )?;
        Ok(())
    }

    pub fn code_expires(&self, email: &str) -> Result<Option<i64>> {
        Ok(self
            .locked()
            .query_row("SELECT expires FROM auth_codes WHERE email = ?1", [email], |row| row.get(0))
            .optional()?)
    }

    pub fn check_code(&self, email: &str, code: &str, now: i64, most_attempts: i64) -> Result<CodeCheck> {
        self.write(|transaction| {
            let kept: Option<(String, i64, i64)> = transaction
                .query_row("SELECT code, expires, attempts FROM auth_codes WHERE email = ?1", [email], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .optional()?;
            let Some((wanted, expires, attempts)) = kept else { return Ok(CodeCheck::Gone) };
            if expires <= now || attempts >= most_attempts {
                transaction.execute("DELETE FROM auth_codes WHERE email = ?1", [email])?;
                return Ok(CodeCheck::Gone);
            }
            if wanted == code {
                transaction.execute("DELETE FROM auth_codes WHERE email = ?1", [email])?;
                return Ok(CodeCheck::Matched);
            }
            transaction.execute("UPDATE auth_codes SET attempts = attempts + 1 WHERE email = ?1", [email])?;
            Ok(CodeCheck::Wrong)
        })
    }

    pub fn put_challenge(&self, id: &str, kind: &str, data: &str, expires: i64) -> Result<()> {
        self.locked().execute(
            "INSERT INTO auth_challenges (id, kind, data, expires) VALUES (?1, ?2, ?3, ?4)",
            params![id, kind, data, expires],
        )?;
        Ok(())
    }

    pub fn take_challenge(&self, id: &str, kind: &str, now: i64) -> Result<Option<String>> {
        self.write(|transaction| {
            let data: Option<String> = transaction
                .query_row(
                    "SELECT data FROM auth_challenges WHERE id = ?1 AND kind = ?2 AND expires > ?3",
                    params![id, kind, now],
                    |row| row.get(0),
                )
                .optional()?;
            transaction.execute("DELETE FROM auth_challenges WHERE id = ?1", [id])?;
            Ok(data)
        })
    }

    pub fn add_passkey(&self, key: &Passkey) -> Result<()> {
        self.locked().execute(
            "INSERT INTO passkeys (id, user, public_key, algorithm, counter, name, created, used)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                key.id,
                key.user.as_str(),
                key.public_key,
                key.algorithm,
                i64::from(key.counter),
                key.name,
                key.created,
                key.used
            ],
        )?;
        Ok(())
    }

    pub fn passkey(&self, id: &str) -> Result<Option<Passkey>> {
        let sql = format!("SELECT {PASSKEY_COLUMNS} FROM passkeys WHERE id = ?1");
        Ok(self.locked().query_row(&sql, [id], passkey).optional()?)
    }

    pub fn passkeys(&self, user: &UserId) -> Result<Vec<Passkey>> {
        let connection = self.locked();
        let sql = format!("SELECT {PASSKEY_COLUMNS} FROM passkeys WHERE user = ?1 ORDER BY created");
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([user.as_str()], passkey)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn used_passkey(&self, id: &str, counter: u32, now: i64) -> Result<()> {
        self.locked().execute(
            "UPDATE passkeys SET counter = ?2, used = ?3 WHERE id = ?1",
            params![id, i64::from(counter), now],
        )?;
        Ok(())
    }

    pub fn remove_passkey(&self, user: &UserId, id: &str) -> Result<bool> {
        let removed =
            self.locked().execute("DELETE FROM passkeys WHERE id = ?1 AND user = ?2", params![id, user.as_str()])?;
        Ok(removed > 0)
    }
}
