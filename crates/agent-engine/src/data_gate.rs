use domain::ModelProvider;
use domain::data_policy::{Handling, Requires, Skipped, joined};

use crate::model::ModelError;

pub fn handling(model: &str, session: ModelProvider) -> Handling {
    let all = crate::providers::all();
    let known: Vec<&str> = all.iter().map(|provider| provider.id()).collect();
    let route = domain::routing::route(model, &known);
    let wanted = route.provider.unwrap_or(session.id());
    let found = all.iter().find(|provider| provider.id() == wanted);
    found.map(|provider| provider.handling(route.model)).unwrap_or_default()
}

pub fn unmet(requires: &Requires, model: &str, session: ModelProvider) -> Option<String> {
    if requires.is_empty() {
        return None;
    }
    let unmet = requires.unmet(&handling(model, session));
    (!unmet.is_empty()).then(|| joined(&unmet))
}

pub fn refused(skipped: &[Skipped]) -> ModelError {
    let listed: Vec<String> = skipped.iter().map(|skipped| format!("{}: {}", skipped.model, skipped.why)).collect();
    ModelError::DataPolicyUnmet(listed.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eu() -> Requires {
        Requires { regions: Some(vec!["eu".into()]), ..Requires::default() }
    }

    #[test]
    fn nothing_required_never_looks_the_model_up() {
        assert_eq!(unmet(&Requires::default(), "anything", ModelProvider::new("nobody")), None);
    }

    #[test]
    fn the_model_is_judged_by_the_provider_that_serves_it() {
        crate::providers::install_testing();
        let keyed = ModelProvider::new("keyed");
        assert_eq!(unmet(&eu(), "m", keyed).as_deref(), Some("region us-east-1 is not eu"));
        assert_eq!(unmet(&eu(), "keyed-too:m", keyed), None);
        assert_eq!(unmet(&eu(), "m", ModelProvider::new("keyed-too")), None);
        assert_eq!(unmet(&eu(), "m", ModelProvider::new("browser-sub")).as_deref(), Some("region unknown is not eu"));
    }

    #[test]
    fn a_refusal_names_every_model_that_was_skipped() {
        let skipped = [
            Skipped { model: "a".into(), why: "region us is not eu".into() },
            Skipped { model: "b".into(), why: "region unknown is not eu".into() },
        ];
        let error = refused(&skipped);
        assert_eq!(error.to_string(), "data_policy_unmet: a: region us is not eu, b: region unknown is not eu");
    }
}
