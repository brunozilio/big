use domain::principal::UserId;
use store::Store;

use crate::roles::{Grant, roles, standing_from};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Whose {
    Everyone,
    Person(String),
}

impl Whose {
    pub fn holds(&self, owner: &str) -> bool {
        match self {
            Self::Everyone => true,
            Self::Person(person) => person == owner,
        }
    }

    pub fn named(&self) -> Option<String> {
        match self {
            Self::Everyone => None,
            Self::Person(person) => Some(person.clone()),
        }
    }
}

pub fn denied(asker: &str, person: &str) -> String {
    format!("{asker} may read only their own work; reading what {person} did needs the admin grant")
}

fn admin(store: &Store, asker: &str) -> bool {
    let user = UserId(asker.to_owned());
    let Ok(Some(principal)) = store.user(&user) else { return false };
    let kept: Vec<String> = match store.roles_of(&user) {
        Ok(held) => held.into_iter().map(|held| held.role).collect(),
        Err(error) => {
            tracing::warn!(%error, "the roles of a person could not be read");
            return false;
        }
    };
    standing_from(roles(), crate::scope::admins().contains(&principal.email), &kept).grants.has(Grant::Admin)
}

fn resolved(store: &Store, person: &str) -> Result<String, String> {
    if !person.contains('@') {
        return Ok(person.to_owned());
    }
    match store.user_by_email(&person.to_lowercase()) {
        Ok(Some(found)) => Ok(found.user.0),
        Ok(None) => Err(format!("no one signs in as {person}")),
        Err(error) => Err(error.to_string()),
    }
}

pub fn whose(store: &Store, shared: bool, asker: &str, person: Option<&str>) -> Result<Whose, String> {
    let wanted = person.map(|person| resolved(store, person)).transpose()?;
    if !shared {
        return Ok(wanted.map_or(Whose::Everyone, Whose::Person));
    }
    let wanted = wanted.unwrap_or_else(|| asker.to_owned());
    if wanted != asker && !admin(store, asker) {
        return Err(denied(asker, person.unwrap_or(&wanted)));
    }
    Ok(Whose::Person(wanted))
}

pub fn may_read(store: &Store, shared: bool, asker: &str, owner: &str) -> bool {
    !shared || asker == owner || admin(store, asker)
}
