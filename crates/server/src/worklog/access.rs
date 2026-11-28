use domain::principal::UserId;
use store::Store;

use crate::roles::{Grant, Grants, Roles, standing_from};

#[derive(Clone, Copy)]
pub struct Reader<'a> {
    pub store: &'a Store,
    pub roles: &'a Roles,
    pub shared: bool,
}

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
    format!(
        "{asker} may read only their own work and their team's; reading what {person} did needs the admin grant, \
         or the team grant and a team in common"
    )
}

fn shown(error: impl std::fmt::Display) -> String {
    error.to_string()
}

struct Standing {
    grants: Grants,
    kept: Vec<String>,
}

fn kept_roles(store: &Store, user: &str) -> Result<Vec<String>, String> {
    let held = store.roles_of(&UserId(user.to_owned())).map_err(shown)?;
    Ok(held.into_iter().map(|held| held.role).collect())
}

fn standing(reader: Reader<'_>, asker: &str) -> Result<Standing, String> {
    let Some(principal) = reader.store.user(&UserId(asker.to_owned())).map_err(shown)? else {
        return Ok(Standing { grants: Grants::default(), kept: Vec::new() });
    };
    let kept = kept_roles(reader.store, asker)?;
    let grants = standing_from(reader.roles, crate::scope::admins().contains(&principal.email), &kept).grants;
    Ok(Standing { grants, kept })
}

pub fn forms_a_team(roles: &Roles, role: &str) -> bool {
    let grants = roles.declared.iter().find(|declared| declared.name == role).map(|declared| declared.grants.clone());
    let leads = grants.is_some_and(|grants| grants.contains(&Grant::Team) || grants.contains(&Grant::Admin));
    !role.is_empty() && role != roles.default_role && !leads
}

pub fn same_team(roles: &Roles, leader: &[String], other: &[String]) -> bool {
    leader.iter().filter(|role| forms_a_team(roles, role)).any(|role| other.contains(role))
}

fn reaches(reader: Reader<'_>, standing: &Standing, owner: &str) -> Result<bool, String> {
    if standing.grants.has(Grant::Admin) {
        return Ok(true);
    }
    if !standing.grants.has(Grant::Team) {
        return Ok(false);
    }
    Ok(same_team(reader.roles, &standing.kept, &kept_roles(reader.store, owner)?))
}

fn resolved(store: &Store, person: &str) -> Result<Option<String>, String> {
    if !person.contains('@') {
        return Ok(Some(person.to_owned()));
    }
    Ok(store.user_by_email(&person.to_lowercase()).map_err(shown)?.map(|found| found.user.0))
}

fn is_asker(store: &Store, asker: &str, person: &str) -> Result<bool, String> {
    if person == asker {
        return Ok(true);
    }
    let own = store.user(&UserId(asker.to_owned())).map_err(shown)?;
    Ok(own.is_some_and(|own| own.email.eq_ignore_ascii_case(person)))
}

pub fn whose(reader: Reader<'_>, asker: &str, person: Option<&str>) -> Result<Whose, String> {
    let store = reader.store;
    if !reader.shared {
        let wanted = person.map(|person| resolved(store, person)).transpose()?;
        return match (person, wanted) {
            (Some(person), Some(None)) => Err(format!("no one signs in as {person}")),
            (_, wanted) => Ok(wanted.flatten().map_or(Whose::Everyone, Whose::Person)),
        };
    }
    let Some(person) = person else { return Ok(Whose::Person(asker.to_owned())) };
    if is_asker(store, asker, person)? {
        return Ok(Whose::Person(asker.to_owned()));
    }
    let standing = standing(reader, asker)?;
    if !standing.grants.has(Grant::Admin) && !standing.grants.has(Grant::Team) {
        return Err(denied(asker, person));
    }
    match resolved(store, person)? {
        Some(owner) if reaches(reader, &standing, &owner)? => Ok(Whose::Person(owner)),
        _ => Err(denied(asker, person)),
    }
}

pub fn may_read(reader: Reader<'_>, asker: &str, owner: &str) -> Result<bool, String> {
    if !reader.shared || asker == owner {
        return Ok(true);
    }
    reaches(reader, &standing(reader, asker)?, owner)
}
