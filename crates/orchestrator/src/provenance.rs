use connectors::contracts::CODE;
use domain::workflow::WorkItem;
use serde_json::{Value, json};
use store::{Application, ConnectionRecord, Store};

pub fn owned(items: &[WorkItem], subject: &str) -> bool {
    items
        .iter()
        .filter(|item| item.ending.is_none())
        .any(|item| holds(&item.input, subject) || item.steps.iter().any(|step| holds(&step.output, subject)))
}

fn holds(value: &Value, subject: &str) -> bool {
    match value {
        Value::String(text) => text == subject,
        Value::Array(list) => list.iter().any(|entry| holds(entry, subject)),
        Value::Object(fields) => fields.values().any(|entry| holds(entry, subject)),
        _ => false,
    }
}

type Found<'a> = (&'a Application, Option<String>);

fn by_scope<'a>(applications: &'a [Application], connection: &str, scope: &str) -> Option<Found<'a>> {
    let held = |bound: &store::Binding| bound.connection == connection && bound.holds(scope);
    applications.iter().find_map(|application| {
        if let Some(repository) =
            application.repositories.iter().find(|repository| repository.bindings.values().any(held))
        {
            return Some((application, Some(repository.id.clone())));
        }
        application.bindings.values().any(held).then_some((application, None))
    })
}

fn by_repo<'a>(applications: &'a [Application], repo: &str) -> Option<Found<'a>> {
    if repo.is_empty() {
        return None;
    }
    applications.iter().find_map(|application| {
        let repository = application
            .repositories
            .iter()
            .find(|repository| repository.bound(CODE).is_some_and(|code| code.holds(repo)))?;
        Some((application, Some(repository.id.clone())))
    })
}

fn by_connection<'a>(applications: &'a [Application], connection: &str) -> Option<Found<'a>> {
    let mut bound = applications
        .iter()
        .filter(|application| application.bindings.values().any(|binding| binding.connection == connection));
    let only = bound.next()?;
    bound.next().is_none().then_some((only, None))
}

pub fn attach(store: &Store, input: &mut Value, event: &Value) {
    let Ok(applications) = store.applications() else { return };
    let data = &event["data"];
    let connection = event["connection"].as_str().unwrap_or_default();
    let scope = event["scope"].as_str().unwrap_or_default();
    let found = if scope.is_empty() {
        by_repo(&applications, data["repo"].as_str().unwrap_or_default())
            .or_else(|| by_connection(&applications, connection))
    } else {
        by_scope(&applications, connection, scope)
    };
    let Some((application, repository)) = found else { return };
    input["application"] = snapshot(store, application);
    let Some(repository) = repository else { return };
    if let Some(source) = input["sources"].get_mut(0) {
        source["repository"] = json!(repository);
    }
    let mut git = json!({});
    for name in ["head", "base"] {
        if let Some(branch) = data[name].as_str().filter(|branch| !branch.is_empty()) {
            git[name] = json!(branch);
        }
    }
    if git.as_object().is_some_and(|fields| !fields.is_empty()) {
        input["git"] = json!({ repository: git });
    }
}

fn described(bindings: &mut Value, records: &[ConnectionRecord]) {
    for binding in bindings.as_object_mut().into_iter().flat_map(|found| found.values_mut()) {
        let connection = binding["connection"].as_str().unwrap_or_default();
        let record = records.iter().find(|record| record.id == connection);
        binding["capabilities"] = json!(record.map(|record| record.capabilities.clone()).unwrap_or_default());
    }
}

pub fn snapshot(store: &Store, application: &Application) -> Value {
    let records = store.connections().unwrap_or_default();
    let mut value = serde_json::to_value(application).unwrap_or(Value::Null);
    if let Some(fields) = value.as_object_mut() {
        fields.remove("env");
    }
    described(&mut value["bindings"], &records);
    for repository in value["repositories"].as_array_mut().into_iter().flatten() {
        described(&mut repository["bindings"], &records);
    }
    value
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use store::{Application, Binding, Bindings, Choice, Store};

    use super::attach;

    fn application(id: &str, binding: Binding) -> Application {
        Application {
            id: id.into(),
            name: id.into(),
            bindings: Bindings::from([("tracker".to_owned(), binding)]),
            ..Application::default()
        }
    }

    fn owner(store: &Store, event: serde_json::Value) -> serde_json::Value {
        let mut input = json!({ "sources": [{}] });
        attach(store, &mut input, &event);
        input["application"]["id"].clone()
    }

    #[test]
    fn a_tracker_event_reaches_the_application_bound_to_its_space_even_on_a_shared_connection() {
        let store = Store::in_memory(0).expect("store");
        let mut ops = Binding::new("c-jira", "OPS");
        ops.more = vec![Choice { value: "SUP".into(), label: String::new() }];
        store.save_application(&application("web", Binding::new("c-jira", "WEB"))).expect("saved");
        store.save_application(&application("ops", ops)).expect("saved");
        assert_eq!(owner(&store, json!({ "connection": "c-jira", "scope": "SUP" })), json!("ops"));
        assert_eq!(owner(&store, json!({ "connection": "c-jira", "scope": "web" })), json!("web"));
        assert_eq!(owner(&store, json!({ "connection": "c-jira", "scope": "OTHER" })), json!(null));
        assert_eq!(owner(&store, json!({ "connection": "c-jira" })), json!(null));
    }

    #[test]
    fn without_a_scope_the_only_application_on_the_connection_still_takes_the_event() {
        let store = Store::in_memory(0).expect("store");
        store.save_application(&application("web", Binding::new("c-jira", "WEB"))).expect("saved");
        assert_eq!(owner(&store, json!({ "connection": "c-jira" })), json!("web"));
    }
}
