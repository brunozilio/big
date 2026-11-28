use serde_json::Value;

pub const ASSIGNED: &str = "assigned";
pub const SSO: &str = "sso";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant {
    Use,
    Author,
    Admin,
}

const ALL: [Grant; 3] = [Grant::Use, Grant::Author, Grant::Admin];

impl Grant {
    pub fn key(self) -> &'static str {
        match self {
            Grant::Use => "use",
            Grant::Author => "author",
            Grant::Admin => "admin",
        }
    }

    fn named(name: &str) -> Option<Grant> {
        ALL.into_iter().find(|grant| grant.key() == name)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Grants {
    held: [bool; 3],
}

impl Grants {
    pub fn everything() -> Self {
        Self { held: [true; 3] }
    }

    fn add(&mut self, grant: Grant) {
        self.held[grant as usize] = true;
    }

    pub fn has(self, grant: Grant) -> bool {
        self.held[Grant::Admin as usize] || self.held[grant as usize]
    }

    pub fn keys(self) -> Vec<&'static str> {
        ALL.into_iter().filter(|grant| self.has(*grant)).map(Grant::key).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub name: String,
    pub grants: Vec<Grant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roles {
    pub declared: Vec<Role>,
    pub default_role: String,
    pub admin_role: String,
    claim: String,
    groups: Vec<(String, String)>,
}

pub struct Access<'a> {
    pub default_role: &'a str,
    pub admin_role: &'a str,
    pub sso_claim: &'a str,
    pub sso_groups: &'a str,
}

fn groups_of(text: &str) -> Vec<(String, String)> {
    text.split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(group, role)| (group.trim().to_owned(), role.trim().to_owned()))
        .filter(|(group, role)| !group.is_empty() && !role.is_empty())
        .collect()
}

fn role_of(name: &str, grants: &str) -> Role {
    let named = grants.split([',', ' ']).filter(|word| !word.trim().is_empty()).filter_map(|word| {
        let found = Grant::named(word.trim());
        if found.is_none() {
            tracing::warn!(role = name, grant = word, "a role names a grant that does not exist");
        }
        found
    });
    Role { name: name.to_owned(), grants: named.collect() }
}

impl Roles {
    pub fn configured() -> Self {
        let access = Access {
            default_role: brand::access::DEFAULT_ROLE,
            admin_role: brand::access::ADMIN_ROLE,
            sso_claim: brand::access::SSO_CLAIM,
            sso_groups: brand::access::SSO_GROUPS,
        };
        Self::of(brand::roles::PAIRS, &access)
    }

    pub fn of(pairs: &[(&str, &str)], access: &Access) -> Self {
        Self {
            declared: pairs.iter().map(|(name, grants)| role_of(name, grants)).collect(),
            default_role: access.default_role.trim().to_owned(),
            admin_role: access.admin_role.trim().to_owned(),
            claim: access.sso_claim.trim().to_owned(),
            groups: groups_of(access.sso_groups),
        }
    }

    pub fn knows(&self, name: &str) -> bool {
        self.declared.iter().any(|role| role.name == name)
    }

    pub fn held(&self, admin: bool, kept: &[String]) -> Vec<String> {
        let mut held: Vec<String> = Vec::new();
        let admin = admin.then(|| self.admin_role.clone());
        for role in admin.into_iter().chain(kept.iter().cloned()) {
            if !role.is_empty() && !held.contains(&role) {
                held.push(role);
            }
        }
        if held.is_empty() && !self.default_role.is_empty() {
            held.push(self.default_role.clone());
        }
        held
    }

    pub fn grants(&self, held: &[String]) -> Grants {
        let mut grants = Grants::default();
        let roles = self.declared.iter().filter(|role| held.contains(&role.name));
        for grant in roles.flat_map(|role| role.grants.iter().copied()) {
            grants.add(grant);
        }
        grants
    }

    pub fn roles_in(&self, claims: &Value) -> Vec<String> {
        let found = self.claim.split('.').filter(|part| !part.is_empty()).fold(claims, |value, part| &value[part]);
        let groups: Vec<&str> = match found {
            Value::String(one) => vec![one.as_str()],
            Value::Array(many) => many.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        let mut roles: Vec<String> = Vec::new();
        for (group, role) in &self.groups {
            if groups.contains(&group.as_str()) && !roles.contains(role) {
                roles.push(role.clone());
            }
        }
        roles
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const PAIRS: &[(&str, &str)] = &[("admin", "use author admin"), ("author", "use, author"), ("member", "use")];

    fn roles(groups: &str) -> Roles {
        let access = Access { default_role: "member", admin_role: "admin", sso_claim: "groups", sso_groups: groups };
        Roles::of(PAIRS, &access)
    }

    #[test]
    fn a_new_person_gets_the_default_role_and_only_use() {
        let roles = roles("");
        let held = roles.held(false, &[]);
        assert_eq!(held, ["member"]);
        let grants = roles.grants(&held);
        assert!(grants.has(Grant::Use));
        assert!(!grants.has(Grant::Author));
        assert!(!grants.has(Grant::Admin));
    }

    #[test]
    fn an_administrator_keeps_every_grant() {
        let roles = roles("");
        let held = roles.held(true, &[]);
        assert_eq!(held, ["admin"]);
        assert_eq!(roles.grants(&held).keys(), ["use", "author", "admin"]);
    }

    #[test]
    fn sso_groups_turn_into_roles() {
        let roles = roles("uso=member, autores=author, plataforma=admin");
        let claims = json!({ "groups": ["uso", "outro"] });
        assert_eq!(roles.roles_in(&claims), ["member"]);
        let claims = json!({ "groups": ["uso", "autores"] });
        assert_eq!(roles.roles_in(&claims), ["member", "author"]);
        assert!(roles.roles_in(&json!({})).is_empty());
    }

    #[test]
    fn the_claim_may_be_nested_or_a_single_text() {
        let access = Access { default_role: "", admin_role: "", sso_claim: "realm.roles", sso_groups: "ops=admin" };
        let roles = Roles::of(PAIRS, &access);
        assert_eq!(roles.roles_in(&json!({ "realm": { "roles": "ops" } })), ["admin"]);
        assert!(roles.held(false, &[]).is_empty());
    }

    #[test]
    fn an_unknown_role_or_grant_gives_nothing() {
        let roles = Roles::of(
            &[("odd", "fly use")],
            &Access { default_role: "", admin_role: "", sso_claim: "", sso_groups: "" },
        );
        assert_eq!(roles.grants(&["odd".to_owned()]).keys(), ["use"]);
        assert!(roles.grants(&["ghost".to_owned()]).keys().is_empty());
        assert!(!roles.knows("ghost"));
    }
}
