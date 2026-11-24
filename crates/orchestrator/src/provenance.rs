use domain::workflow::WorkItem;
use serde_json::{Value, json};
use store::{Application, Store};

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

pub fn attach(store: &Store, input: &mut Value, event: &Value) {
    let Ok(applications) = store.applications() else { return };
    let data = &event["data"];
    let repo = data["repo"].as_str().unwrap_or_default();
    let by_repo = applications.iter().find_map(|application| {
        let repository = application
            .repositories
            .iter()
            .find(|repository| repository.code.as_ref().is_some_and(|code| !repo.is_empty() && code.repo == repo))?;
        Some((application, Some(repository.id.clone())))
    });
    let connection = event["connection"].as_str().unwrap_or_default();
    let by_tracker = || {
        applications
            .iter()
            .find(|application| application.tracker.as_ref().is_some_and(|tracker| tracker.connection == connection))
            .map(|application| (application, None))
    };
    let Some((application, repository)) = by_repo.or_else(by_tracker) else { return };
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

pub fn snapshot(store: &Store, application: &Application) -> Value {
    let records = store.connections().unwrap_or_default();
    let mut value = serde_json::to_value(application).unwrap_or(Value::Null);
    for repository in value["repositories"].as_array_mut().into_iter().flatten() {
        let Some(connection) = repository["code"]["connection"].as_str().map(str::to_owned) else { continue };
        let record = records.iter().find(|record| record.id == connection);
        let pipelines = record
            .is_some_and(|record| record.capabilities.iter().any(|name| name == connectors::contracts::PIPELINES));
        repository["code"]["pipelines"] = json!(pipelines);
    }
    value
}
