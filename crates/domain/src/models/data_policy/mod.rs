use std::fmt;

use serde::{Deserialize, Serialize};

pub const SECTION: &str = "data_policy";
pub const UNMET: &str = "data_policy_unmet";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handling {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Requires {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_training: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unmet {
    Region { wanted: Vec<String>, declared: Option<String> },
    Training { declared: Option<bool> },
    Retention { most: u32, declared: Option<u32> },
}

impl fmt::Display for Unmet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Region { wanted, .. } if wanted.is_empty() => f.write_str("no region meets every requirement"),
            Self::Region { wanted, declared } => {
                write!(f, "region {} is not {}", declared.as_deref().unwrap_or("unknown"), wanted.join(" or "))
            }
            Self::Training { declared: Some(_) } => f.write_str("trains on the data"),
            Self::Training { declared: None } => f.write_str("training on the data is unknown"),
            Self::Retention { most, declared: Some(days) } => write!(f, "keeps data {days} days, more than {most}"),
            Self::Retention { declared: None, .. } => f.write_str("retention is unknown"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    pub model: String,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rerouted {
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    pub failure: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<Skipped>,
}

fn within(declared: &str, wanted: &str) -> bool {
    let (declared, wanted) = (declared.to_ascii_lowercase(), wanted.to_ascii_lowercase());
    declared == wanted || declared.strip_prefix(&wanted).is_some_and(|rest| rest.starts_with('-'))
}

fn narrowed(one: &[String], other: &[String]) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for (a, b) in one.iter().flat_map(|a| other.iter().map(move |b| (a, b))) {
        let found = if within(b, a) { Some(b) } else { within(a, b).then_some(a) };
        if let Some(found) = found.filter(|found| !kept.contains(found)) {
            kept.push(found.clone());
        }
    }
    kept
}

pub fn joined(unmet: &[Unmet]) -> String {
    unmet.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}

impl Requires {
    pub fn configured() -> Self {
        Self::read(brand::section(SECTION))
    }

    pub fn read(pairs: &[(&str, &str)]) -> Self {
        let value = |key: &str| pairs.iter().find(|(name, _)| *name == key).map(|(_, value)| value.trim());
        let regions: Vec<String> = value("region").unwrap_or_default().split_whitespace().map(str::to_owned).collect();
        Self {
            regions: (!regions.is_empty()).then_some(regions),
            no_training: value("training").is_some_and(|training| training.eq_ignore_ascii_case("no")),
            retention_days: value("retention_days").and_then(|days| days.parse().ok()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_none() && !self.no_training && self.retention_days.is_none()
    }

    pub fn stricter(self, other: &Requires) -> Requires {
        let regions = match (self.regions, &other.regions) {
            (Some(mine), Some(theirs)) => Some(narrowed(&mine, theirs)),
            (mine, theirs) => mine.or_else(|| theirs.clone()),
        };
        let retention_days = match (self.retention_days, other.retention_days) {
            (Some(mine), Some(theirs)) => Some(mine.min(theirs)),
            (mine, theirs) => mine.or(theirs),
        };
        Requires { regions, no_training: self.no_training || other.no_training, retention_days }
    }

    pub fn unmet(&self, handling: &Handling) -> Vec<Unmet> {
        let mut unmet = Vec::new();
        if let Some(wanted) = &self.regions {
            let declared = handling.region.as_deref();
            if !declared.is_some_and(|declared| wanted.iter().any(|wanted| within(declared, wanted))) {
                unmet.push(Unmet::Region { wanted: wanted.clone(), declared: declared.map(str::to_owned) });
            }
        }
        if self.no_training && handling.training != Some(false) {
            unmet.push(Unmet::Training { declared: handling.training });
        }
        if let Some(most) = self.retention_days
            && !handling.retention_days.is_some_and(|days| days <= most)
        {
            unmet.push(Unmet::Retention { most, declared: handling.retention_days });
        }
        unmet
    }
}

#[cfg(test)]
mod tests;
