use domain::model_roles::{ModelRoles, SMALL};

use crate::prompt::settings::Settings;

pub fn merged(asked: &ModelRoles, settings: &Settings) -> ModelRoles {
    let mut roles = ModelRoles::default();
    roles.set(SMALL, settings.small_model.clone());
    for (role, model) in asked.iter() {
        roles.set(role, Some(model.to_owned()));
    }
    roles
}

pub fn fallbacks(session: &domain::SessionConfig, settings: &[String]) -> Vec<String> {
    match (&session.fallback_model, session.fallbacks.is_empty()) {
        (Some(model), _) => vec![model.clone()],
        (None, false) => session.fallbacks.clone(),
        (None, true) => settings.to_vec(),
    }
}

pub fn elsewhere(model: &str, current: domain::ModelProvider) -> bool {
    let known: Vec<&str> = crate::providers::all().iter().map(|provider| provider.id()).collect();
    domain::routing::route(model, &known).provider.is_some_and(|provider| provider != current.id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::model_roles::PLAN;

    #[test]
    fn what_the_session_asks_for_wins_over_the_settings_files() {
        let settings = Settings { small_model: Some("haiku".into()), ..Settings::default() };
        let mut asked = ModelRoles::default();
        asked.set(SMALL, Some("grok-mini".into()));
        asked.set(PLAN, Some("opus".into()));
        let roles = merged(&asked, &settings);
        let all: Vec<(&str, &str)> = roles.iter().collect();
        assert_eq!(all, [(PLAN, "opus"), (SMALL, "grok-mini")]);
    }
}
