use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Shadow,
    Enforce,
}

impl Mode {
    pub fn word(self) -> &'static str {
        match self {
            Self::Shadow => "shadow",
            Self::Enforce => "enforce",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Levels {
    pub mode: Mode,
    pub levels: Vec<String>,
}

impl Levels {
    pub fn of(value: &Value) -> Result<Levels, String> {
        if value.is_null() {
            return Ok(Levels::default());
        }
        let levels: Levels = serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
        let mut seen: Vec<String> = Vec::new();
        for level in &levels.levels {
            let named = level.trim().to_lowercase();
            if named.is_empty() {
                return Err("a level has no name".into());
            }
            if seen.contains(&named) {
                return Err(format!("the level {level} is declared twice"));
            }
            seen.push(named);
        }
        Ok(levels)
    }

    pub fn enforced(&self) -> bool {
        self.mode == Mode::Enforce && !self.levels.is_empty()
    }

    pub fn rank(&self, label: &str) -> Option<usize> {
        let wanted = label.trim();
        self.levels.iter().position(|level| level.trim().eq_ignore_ascii_case(wanted))
    }

    pub fn most_restricted(&self) -> Option<&str> {
        self.levels.last().map(String::as_str)
    }

    pub fn ranked(&self, label: Option<&str>) -> Option<usize> {
        let known = label.and_then(|label| self.rank(label));
        if known.is_some() || !self.enforced() {
            return known;
        }
        self.levels.len().checked_sub(1)
    }

    pub fn ceiling(&self, ceiling: &str) -> Option<usize> {
        let known = self.rank(ceiling);
        if known.is_none() && self.enforced() { Some(0) } else { known }
    }
}
