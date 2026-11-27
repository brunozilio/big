use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::approval::PermissionMode;

pub const PLAN: &str = "plan";
pub const EXPLORE: &str = "explore";
pub const SMALL: &str = "small";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelRole(String);

impl ModelRole {
    pub fn named(name: &str) -> Self {
        Self(name.trim().to_ascii_lowercase())
    }

    pub fn name(&self) -> &str {
        &self.0
    }

    pub fn parse(text: &str) -> Option<Self> {
        Tiers::declared().role(text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierSpec {
    pub name: String,
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default)]
    pub session: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tiers(Vec<TierSpec>);

static DECLARED: RwLock<Option<Arc<Tiers>>> = RwLock::new(None);

impl Tiers {
    pub fn read(value: &Value) -> Self {
        let listed = value.as_array().map(Vec::as_slice).unwrap_or_default();
        let mut kept: Vec<TierSpec> = Vec::new();
        for entry in listed {
            let Ok(mut spec) = serde_json::from_value::<TierSpec>(entry.clone()) else { continue };
            spec.name = spec.name.trim().to_ascii_lowercase();
            spec.fallback = spec.fallback.map(|next| next.trim().to_ascii_lowercase()).filter(|next| !next.is_empty());
            if spec.name.is_empty() || kept.iter().any(|known| known.name == spec.name) {
                continue;
            }
            kept.push(spec);
        }
        Self(kept)
    }

    pub fn all(&self) -> &[TierSpec] {
        &self.0
    }

    pub fn find(&self, name: &str) -> Option<&TierSpec> {
        let wanted = name.trim();
        self.0.iter().find(|spec| spec.name.eq_ignore_ascii_case(wanted))
    }

    pub fn roles(&self) -> impl Iterator<Item = &TierSpec> {
        self.0.iter().filter(|spec| !spec.session)
    }

    pub fn role(&self, text: &str) -> Option<ModelRole> {
        self.find(text).filter(|spec| !spec.session).map(|spec| ModelRole::named(&spec.name))
    }

    pub fn declare(tiers: Arc<Tiers>) {
        *DECLARED.write().unwrap_or_else(PoisonError::into_inner) = Some(tiers);
    }

    pub fn declared() -> Arc<Tiers> {
        DECLARED.read().unwrap_or_else(PoisonError::into_inner).clone().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelRoles(BTreeMap<ModelRole, String>);

impl ModelRoles {
    pub fn get(&self, role: &str) -> Option<&str> {
        self.0.get(&ModelRole::named(role)).map(String::as_str)
    }

    pub fn set(&mut self, role: &str, model: Option<String>) {
        let role = ModelRole::named(role);
        match model.map(|name| name.trim().to_owned()).filter(|name| !name.is_empty()) {
            Some(model) => self.0.insert(role, model),
            None => self.0.remove(&role),
        };
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(role, model)| (role.name(), model.as_str()))
    }

    pub fn for_turn<'a>(&'a self, session: &'a str, permission: PermissionMode) -> &'a str {
        match permission {
            PermissionMode::Plan => self.get(PLAN).unwrap_or(session),
            _ => session,
        }
    }

    pub fn for_agent<'a>(&'a self, agent: &str, small: Option<&'a str>) -> Option<&'a str> {
        match agent {
            "Plan" => self.get(PLAN),
            "Explore" => self.get(EXPLORE).or(small),
            _ => None,
        }
    }

    pub fn for_tier<'a>(&'a self, tiers: &Tiers, tier: &str, session: &'a str, small: Option<&'a str>) -> &'a str {
        let own = |name: &str| if name == SMALL { small.or_else(|| self.get(SMALL)) } else { self.get(name) };
        let mut current = tiers.find(tier);
        for _ in 0..=tiers.all().len() {
            let Some(spec) = current else { break };
            if spec.session {
                return session;
            }
            if let Some(model) = own(&spec.name) {
                return model;
            }
            current = spec.fallback.as_deref().and_then(|next| tiers.find(next));
        }
        session
    }
}

#[cfg(test)]
mod tests;
