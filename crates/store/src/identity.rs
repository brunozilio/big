use database::{Row, args};
use domain::principal::{Principal, UserId};

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

fn principal(row: &Row) -> database::Result<Principal> {
    Ok(Principal { user: UserId(row.get(0)?), email: row.get(1)?, name: row.get(2)? })
}

fn passkey(row: &Row) -> database::Result<Passkey> {
    Ok(Passkey {
        id: row.get(0)?,
        user: UserId(row.get(1)?),
        public_key: row.get(2)?,
        algorithm: row.get(3)?,
        counter: u32::try_from(row.get::<i64>(4)?).unwrap_or(u32::MAX),
        name: row.get(5)?,
        created: row.get(6)?,
        used: row.get(7)?,
    })
}

const PASSKEY_COLUMNS: &str = "id, \"user\", public_key, algorithm, counter, name, created, used";

impl Store {
    fn principal_where(&self, sql: &str, key: &str) -> Result<Option<Principal>> {
        self.read(|conn| Ok(conn.row(sql, args![key])?.as_ref().map(principal).transpose()?))
    }

    pub fn user_by_email(&self, email: &str) -> Result<Option<Principal>> {
        self.principal_where("SELECT id, email, name FROM users WHERE email = ?1", email)
    }

    pub fn user_ids(&self) -> Result<Vec<String>> {
        self.read(|conn| Ok(conn.all("SELECT id FROM users", &[])?))
    }

    pub fn user(&self, id: &UserId) -> Result<Option<Principal>> {
        self.principal_where("SELECT id, email, name FROM users WHERE id = ?1", id.as_str())
    }

    pub fn ensure_user(&self, wanted: &Principal, now: i64) -> Result<Principal> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO users (id, email, name, created, seen) VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT (email) DO UPDATE SET seen = excluded.seen,
                   name = CASE WHEN excluded.name = '' THEN users.name ELSE excluded.name END",
                args![wanted.user.as_str(), wanted.email, wanted.name, now],
            )?;
            let row = conn.row("SELECT id, email, name FROM users WHERE email = ?1", args![wanted.email])?;
            let row = row.ok_or_else(|| database::Error::decode("the user just written is missing"))?;
            Ok(principal(&row)?)
        })
    }

    pub fn open_session(&self, token: &str, user: &UserId, now: i64, expires: i64) -> Result<()> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO auth_sessions (token, \"user\", created, expires) VALUES (?1, ?2, ?3, ?4)",
                args![token, user.as_str(), now, expires],
            )?;
            Ok(())
        })
    }

    pub fn session_user(&self, token: &str, now: i64) -> Result<Option<Principal>> {
        self.read(|conn| {
            let row = conn.row(
                "SELECT u.id, u.email, u.name FROM auth_sessions s JOIN users u ON u.id = s.\"user\"
                 WHERE s.token = ?1 AND s.expires > ?2",
                args![token, now],
            )?;
            Ok(row.as_ref().map(principal).transpose()?)
        })
    }

    pub fn people(&self, now: i64) -> Result<Vec<Person>> {
        self.read(|conn| {
            let rows = conn.query(
                "SELECT u.id, u.email, u.name, u.created, u.seen,
                        (SELECT COUNT(*) FROM auth_sessions s WHERE s.\"user\" = u.id AND s.expires > ?1)
                 FROM users u ORDER BY u.email",
                args![now],
            )?;
            let people = rows.iter().map(|row| {
                Ok(Person {
                    principal: principal(row)?,
                    created: row.get(3)?,
                    seen: row.get(4)?,
                    sessions: row.get(5)?,
                })
            });
            Ok(people.collect::<database::Result<_>>()?)
        })
    }

    pub fn close_sessions_of(&self, user: &UserId) -> Result<usize> {
        let closed = self
            .write(|conn| Ok(conn.execute("DELETE FROM auth_sessions WHERE \"user\" = ?1", args![user.as_str()])?))?;
        Ok(usize::try_from(closed).unwrap_or(usize::MAX))
    }

    pub fn close_session(&self, token: &str) -> Result<()> {
        self.write(|conn| {
            conn.execute("DELETE FROM auth_sessions WHERE token = ?1", args![token])?;
            Ok(())
        })
    }

    pub fn forget_expired(&self, now: i64) -> Result<()> {
        self.write(|conn| {
            conn.execute("DELETE FROM auth_sessions WHERE expires <= ?1", args![now])?;
            conn.execute("DELETE FROM auth_codes WHERE expires <= ?1", args![now])?;
            conn.execute("DELETE FROM auth_challenges WHERE expires <= ?1", args![now])?;
            Ok(())
        })
    }

    pub fn put_code(&self, email: &str, code: &str, expires: i64) -> Result<()> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO auth_codes (email, code, expires, attempts) VALUES (?1, ?2, ?3, 0)
                 ON CONFLICT (email) DO UPDATE SET code = excluded.code, expires = excluded.expires, attempts = 0",
                args![email, code, expires],
            )?;
            Ok(())
        })
    }

    pub fn code_expires(&self, email: &str) -> Result<Option<i64>> {
        self.read(|conn| Ok(conn.one("SELECT expires FROM auth_codes WHERE email = ?1", args![email])?))
    }

    pub fn check_code(&self, email: &str, code: &str, now: i64, most_attempts: i64) -> Result<CodeCheck> {
        self.write(|conn| {
            let kept = conn.row("SELECT code, expires, attempts FROM auth_codes WHERE email = ?1", args![email])?;
            let Some(kept) = kept else { return Ok(CodeCheck::Gone) };
            let (wanted, expires, attempts): (String, i64, i64) = (kept.get(0)?, kept.get(1)?, kept.get(2)?);
            if expires <= now || attempts >= most_attempts {
                conn.execute("DELETE FROM auth_codes WHERE email = ?1", args![email])?;
                return Ok(CodeCheck::Gone);
            }
            if wanted == code {
                conn.execute("DELETE FROM auth_codes WHERE email = ?1", args![email])?;
                return Ok(CodeCheck::Matched);
            }
            conn.execute("UPDATE auth_codes SET attempts = attempts + 1 WHERE email = ?1", args![email])?;
            Ok(CodeCheck::Wrong)
        })
    }

    pub fn put_challenge(&self, id: &str, kind: &str, data: &str, expires: i64) -> Result<()> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO auth_challenges (id, kind, data, expires) VALUES (?1, ?2, ?3, ?4)",
                args![id, kind, data, expires],
            )?;
            Ok(())
        })
    }

    pub fn take_challenge(&self, id: &str, kind: &str, now: i64) -> Result<Option<String>> {
        self.write(|conn| {
            let data: Option<String> = conn.one(
                "SELECT data FROM auth_challenges WHERE id = ?1 AND kind = ?2 AND expires > ?3",
                args![id, kind, now],
            )?;
            conn.execute("DELETE FROM auth_challenges WHERE id = ?1", args![id])?;
            Ok(data)
        })
    }

    pub fn add_passkey(&self, key: &Passkey) -> Result<()> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO passkeys (id, \"user\", public_key, algorithm, counter, name, created, used)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                args![
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
        })
    }

    pub fn passkey(&self, id: &str) -> Result<Option<Passkey>> {
        let sql = format!("SELECT {PASSKEY_COLUMNS} FROM passkeys WHERE id = ?1");
        self.read(|conn| Ok(conn.row(&sql, args![id])?.as_ref().map(passkey).transpose()?))
    }

    pub fn passkeys(&self, user: &UserId) -> Result<Vec<Passkey>> {
        let sql = format!("SELECT {PASSKEY_COLUMNS} FROM passkeys WHERE \"user\" = ?1 ORDER BY created");
        self.read(|conn| {
            Ok(conn.query(&sql, args![user.as_str()])?.iter().map(passkey).collect::<database::Result<_>>()?)
        })
    }

    pub fn used_passkey(&self, id: &str, counter: u32, now: i64) -> Result<()> {
        self.write(|conn| {
            conn.execute(
                "UPDATE passkeys SET counter = ?2, used = ?3 WHERE id = ?1",
                args![id, i64::from(counter), now],
            )?;
            Ok(())
        })
    }

    pub fn remove_passkey(&self, user: &UserId, id: &str) -> Result<bool> {
        let removed = self.write(|conn| {
            Ok(conn.execute("DELETE FROM passkeys WHERE id = ?1 AND \"user\" = ?2", args![id, user.as_str()])?)
        })?;
        Ok(removed > 0)
    }
}
