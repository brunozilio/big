use connectors::Connectors;
use domain::connection::ProviderKey;
use domain::data_policy::Requires;
use serde_json::Value;
use store::Store;

fn asked(args: &Value) -> Requires {
    let given = &args["requires"];
    if given.is_null() {
        return Requires::default();
    }
    serde_json::from_value(given.clone()).unwrap_or_else(|error| {
        tracing::warn!(%error, "the step's requires is not valid and is left out");
        Requires::default()
    })
}

pub fn requires(store: &Store, hub: &Connectors, input: &Value, args: &Value) -> Requires {
    let asked = asked(args);
    let found = crate::application::id_of(input).and_then(|id| store.application(&id).ok().flatten());
    let Some(application) = found else { return asked };
    let declared = application
        .connections()
        .into_iter()
        .filter_map(|id| store.connection(id).ok().flatten())
        .map(|record| hub.requires(&ProviderKey::from(record.provider.as_str())));
    declared.fold(application.requires.clone().stricter(&asked), |all, one| all.stricter(&one))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_step_asks_with_the_same_shape_the_space_stores() {
        let wanted = Requires { regions: Some(vec!["eu".into()]), no_training: true, retention_days: None };
        assert_eq!(asked(&json!({ "requires": { "regions": ["eu"], "noTraining": true } })), wanted);
        assert!(asked(&json!({})).is_empty());
        assert!(asked(&json!({ "requires": "eu" })).is_empty());
    }
}
