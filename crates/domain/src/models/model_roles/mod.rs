use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::approval::PermissionMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelRole {
    Plan,
    Explore,
    Small,
}

impl ModelRole {
    pub const ALL: [Self; 3] = [Self::Plan, Self::Explore, Self::Small];

    pub fn name(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Explore => "explore",
            Self::Small => "small",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Self::Plan => "plan mode and the Plan agent",
            Self::Explore => "the Explore agent that searches the code",
            Self::Small => "background chores: titles, effort picks, web reading",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let wanted = text.trim();
        Self::ALL.into_iter().find(|role| role.name().eq_ignore_ascii_case(wanted))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelRoles(BTreeMap<ModelRole, String>);

impl ModelRoles {
    pub fn get(&self, role: ModelRole) -> Option<&str> {
        self.0.get(&role).map(String::as_str)
    }

    pub fn set(&mut self, role: ModelRole, model: Option<String>) {
        match model.map(|name| name.trim().to_owned()).filter(|name| !name.is_empty()) {
            Some(model) => self.0.insert(role, model),
            None => self.0.remove(&role),
        };
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ModelRole, &str)> {
        self.0.iter().map(|(role, model)| (*role, model.as_str()))
    }

    pub fn for_turn<'a>(&'a self, session: &'a str, permission: PermissionMode) -> &'a str {
        match permission {
            PermissionMode::Plan => self.get(ModelRole::Plan).unwrap_or(session),
            _ => session,
        }
    }

    pub fn for_agent<'a>(&'a self, agent: &str, small: Option<&'a str>) -> Option<&'a str> {
        match agent {
            "Plan" => self.get(ModelRole::Plan),
            "Explore" => self.get(ModelRole::Explore).or(small),
            _ => None,
        }
    }

    pub fn for_tier<'a>(&'a self, tier: Tier, session: &'a str, small: Option<&'a str>) -> &'a str {
        match tier {
            Tier::Main => session,
            Tier::Plan => self.get(ModelRole::Plan).unwrap_or(session),
            Tier::Explore => self.get(ModelRole::Explore).or(small).unwrap_or(session),
            Tier::Small => small.or_else(|| self.get(ModelRole::Small)).unwrap_or(session),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Main,
    Plan,
    Explore,
    Small,
}

impl Tier {
    pub fn parse(text: &str) -> Option<Self> {
        if text.trim().eq_ignore_ascii_case("main") {
            return Some(Self::Main);
        }
        ModelRole::parse(text).map(|role| match role {
            ModelRole::Plan => Self::Plan,
            ModelRole::Explore => Self::Explore,
            ModelRole::Small => Self::Small,
        })
    }
}

#[cfg(test)]
mod tests;
