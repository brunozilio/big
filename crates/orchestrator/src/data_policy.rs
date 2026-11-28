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
    use std::sync::Arc;

    use connectors::scripted::ScriptedProvider;
    use connectors::{FakeHttp, FixedClock, MemoryTokenStore, Provider, TokenKeeper};
    use serde_json::json;
    use store::{Application, Binding, ConnectionRecord};

    use super::*;

    const STRICT: &str = r#"export function provider() {
  return { schema_version: 2, name: "Strict", capabilities: ["tracker"], auth: { kind: "token", fields: [] }, actions: [],
    requires: { regions: ["eu-central-1", "eu-west-1"], retentionDays: 30 } };
}
export function call() { return null; }
"#;

    fn hub(folder: &std::path::Path) -> Connectors {
        let dir = folder.join("connectors/strict");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("index.ts"), STRICT).expect("index");
        let loaded = ScriptedProvider::load_in(Some(folder), "connectors/strict/index.ts").expect("loaded");
        let keeper = TokenKeeper::new(
            Arc::new(MemoryTokenStore::new()),
            Arc::new(FakeHttp::new()),
            Arc::new(FixedClock::at_second(1)),
        );
        let providers: Vec<Arc<dyn Provider>> = vec![Arc::new(loaded)];
        Connectors::from_providers(providers, Arc::new(FakeHttp::new()), keeper)
    }

    fn space(store: &Store) {
        let record = ConnectionRecord {
            id: "c-strict".into(),
            provider: "strict".into(),
            label: "Strict".into(),
            account: None,
            capabilities: vec!["tracker".into()],
            settings: json!({}),
        };
        store.save_connection(&record).expect("connection");
        let mut application = Application { id: "a1".into(), name: "Web".into(), ..Application::default() };
        application.bindings.insert("tracker".into(), Binding::new("c-strict", "APP"));
        application.requires =
            Requires { regions: Some(vec!["eu".into()]), retention_days: Some(90), ..Requires::default() };
        store.save_application(&application).expect("application");
    }

    #[test]
    fn the_config_the_space_its_connectors_and_the_step_add_up_to_the_strictest() {
        let folder = tempfile::tempdir().expect("folder");
        let store = Store::open(&folder.path().join("app.db"), 0).expect("store");
        space(&store);
        let hub = hub(folder.path());
        let input = json!({ "application": { "id": "a1" } });
        let args = json!({ "requires": { "regions": ["eu-central-1"], "noTraining": true, "retentionDays": 60 } });
        let found = requires(&store, &hub, &input, &args);
        let config = Requires::read(&[("region", "eu local"), ("training", ""), ("retention_days", "7")]);
        let wanted =
            Requires { regions: Some(vec!["eu-central-1".into()]), no_training: true, retention_days: Some(7) };
        assert_eq!(config.stricter(&found), wanted);
        let alone = requires(&store, &hub, &json!({}), &args);
        assert_eq!(alone, asked(&args));
        let unbound = requires(&store, &hub, &json!({ "application": { "id": "a1" } }), &json!({}));
        assert_eq!(unbound.regions, Some(vec!["eu-central-1".into(), "eu-west-1".into()]));
        assert_eq!(unbound.retention_days, Some(30));
    }

    #[test]
    fn a_step_asks_with_the_same_shape_the_space_stores() {
        let wanted = Requires { regions: Some(vec!["eu".into()]), no_training: true, retention_days: None };
        assert_eq!(asked(&json!({ "requires": { "regions": ["eu"], "noTraining": true } })), wanted);
        assert!(asked(&json!({})).is_empty());
        assert!(asked(&json!({ "requires": "eu" })).is_empty());
    }
}
