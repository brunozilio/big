use domain::principal::Principal;
use domain::workflow::Assignee;

use crate::roles::{Grant, Roles, Standing};
use crate::state::Shared;

pub struct Reader<'a> {
    pub person: &'a Principal,
    pub standing: Standing,
    pub everyone: bool,
}

impl<'a> Reader<'a> {
    pub fn of(shared: &Shared, person: &'a Principal) -> Self {
        let everyone = !crate::scope::shared_mode();
        Self { person, standing: crate::roles::standing(shared, person), everyone }
    }

    pub fn reaches(&self, assign: &Assignee) -> bool {
        self.everyone || reaches(crate::roles::roles(), self.person, &self.standing, assign)
    }

    pub fn owns(&self, owner: &str) -> bool {
        self.everyone || owner == self.person.owner()
    }

    pub fn sees(&self, owner: &str, assign: Option<&Assignee>) -> bool {
        assign.map_or_else(|| self.owns(owner), |assign| self.reaches(assign))
    }

    pub fn admin(&self) -> bool {
        self.everyone || self.standing.grants.has(Grant::Admin)
    }

    pub fn display(&self) -> &str {
        [self.person.name.trim(), self.person.email.trim()].into_iter().find(|name| !name.is_empty()).unwrap_or("human")
    }
}

pub fn reaches(roles: &Roles, person: &Principal, standing: &Standing, assign: &Assignee) -> bool {
    let holds = |role: &String| standing.roles.contains(role);
    match assign {
        Assignee::Person(who) => {
            [person.email.as_str(), person.owner()].iter().any(|own| own.eq_ignore_ascii_case(who))
        }
        Assignee::Role(role) => holds(role),
        Assignee::Group(group) => roles.of_group(group).iter().any(holds),
        Assignee::Admins => standing.grants.has(Grant::Admin),
    }
}

#[cfg(test)]
mod tests {
    use domain::principal::UserId;

    use super::*;
    use crate::roles::{Access, standing_from};

    fn roles() -> Roles {
        let access =
            Access { default_role: "member", admin_role: "admin", sso_claim: "groups", sso_groups: "fin=controller" };
        Roles::of(&[("admin", "use author admin"), ("controller", "use"), ("member", "use")], &access)
    }

    fn ana() -> Principal {
        Principal { user: UserId("u-ana".into()), email: "Ana@Example.com".into(), name: "Ana".into() }
    }

    #[test]
    fn a_role_a_group_a_person_and_the_admins_each_reach_their_own() {
        let roles = roles();
        let controller = standing_from(&roles, false, &["controller".to_owned()]);
        let member = standing_from(&roles, false, &[]);
        let admin = standing_from(&roles, true, &[]);
        let role = Assignee::Role("controller".into());
        assert!(reaches(&roles, &ana(), &controller, &role));
        assert!(!reaches(&roles, &ana(), &member, &role));
        assert!(reaches(&roles, &ana(), &controller, &Assignee::Group("fin".into())));
        assert!(!reaches(&roles, &ana(), &member, &Assignee::Group("fin".into())));
        assert!(reaches(&roles, &ana(), &member, &Assignee::Person("ana@example.com".into())));
        assert!(reaches(&roles, &ana(), &member, &Assignee::Person("u-ana".into())));
        assert!(!reaches(&roles, &ana(), &member, &Assignee::Person("bia@example.com".into())));
        assert!(reaches(&roles, &ana(), &admin, &Assignee::Admins));
        assert!(!reaches(&roles, &ana(), &controller, &Assignee::Admins));
    }
}
