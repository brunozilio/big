use serde_json::Value;

use crate::quota::Scope;

const SECTION: &str = "budget";

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Ceilings {
    pub run: Option<f64>,
    pub agent: Option<f64>,
    pub application: Option<f64>,
    pub person: Option<f64>,
}

pub fn dollars(text: &str) -> Option<f64> {
    text.trim().trim_start_matches('$').parse::<f64>().ok().filter(|usd| usd.is_finite() && *usd > 0.0)
}

pub fn declared(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64().filter(|usd| usd.is_finite() && *usd > 0.0),
        Value::String(text) => dollars(text),
        _ => None,
    }
}

impl Ceilings {
    pub fn configured() -> Self {
        Self::read(brand::section(SECTION))
    }

    pub fn read(pairs: &[(&str, &str)]) -> Self {
        let value = |key: &str| pairs.iter().find(|(name, _)| *name == key).and_then(|(_, value)| dollars(value));
        Self {
            run: value("run_usd"),
            agent: value("agent_usd"),
            application: value("application_usd"),
            person: value("person_usd"),
        }
    }

    pub const fn of(&self, scope: Scope) -> Option<f64> {
        match scope {
            Scope::Run => self.run,
            Scope::Agent => self.agent,
            Scope::Application => self.application,
            Scope::Person => self.person,
            Scope::Session | Scope::Item | Scope::Account | Scope::Workspace => None,
        }
    }

    pub fn run_of(&self, step: &Value, agent: Option<f64>) -> Option<f64> {
        declared(step).or(agent).or(self.run)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_ceiling_left_empty_or_zero_is_off_and_the_shipped_config_starts_with_all_off() {
        assert_eq!(Ceilings::read(&[]), Ceilings::default());
        let read =
            Ceilings::read(&[("run_usd", "2.5"), ("agent_usd", ""), ("application_usd", "0"), ("person_usd", "$40")]);
        assert_eq!(read, Ceilings { run: Some(2.5), agent: None, application: None, person: Some(40.0) });
        assert_eq!(read.of(Scope::Person), Some(40.0));
        assert_eq!(read.of(Scope::Session), None);
        assert_eq!(Ceilings::configured(), Ceilings::default(), "no ceiling is on until someone sets it");
    }

    #[test]
    fn a_run_s_ceiling_is_the_step_s_then_the_agent_s_then_the_config_s() {
        let config = Ceilings { run: Some(5.0), ..Ceilings::default() };
        assert_eq!(config.run_of(&json!(1.5), Some(3.0)), Some(1.5));
        assert_eq!(config.run_of(&json!("2"), None), Some(2.0));
        assert_eq!(config.run_of(&Value::Null, Some(3.0)), Some(3.0));
        assert_eq!(config.run_of(&json!(-1), None), Some(5.0));
        assert_eq!(Ceilings::default().run_of(&Value::Null, None), None);
    }
}
