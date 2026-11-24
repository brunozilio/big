use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use platform::sync::Locked;
use serde_json::Value;

use super::{Tool, Workspace};

pub type Handler = Arc<dyn Fn(&Value, &Workspace) -> Result<Value, String> + Send + Sync>;

type Offered = Vec<(String, Handler)>;

fn lock() -> MutexGuard<'static, HashMap<u64, Offered>> {
    static ALL: OnceLock<Mutex<HashMap<u64, Offered>>> = OnceLock::new();
    ALL.get_or_init(Mutex::default).locked()
}

pub fn offer(client: u64, name: &str, handler: Handler) {
    let mut all = lock();
    let offered = all.entry(client).or_default();
    offered.retain(|(known, _)| known != name);
    offered.push((name.to_owned(), handler));
}

pub fn tools(client: u64) -> Vec<Box<dyn Tool>> {
    let names: Vec<String> =
        lock().get(&client).map(|offered| offered.iter().map(|(name, _)| name.clone()).collect()).unwrap_or_default();
    names
        .iter()
        .filter_map(|name| super::conditional::tool(name, &Value::Null))
        .map(|tool| Box::new(tool) as Box<dyn Tool>)
        .collect()
}

pub fn call(client: u64, name: &str, input: &Value, workspace: &Workspace) -> Result<Value, String> {
    let handler = lock()
        .get(&client)
        .and_then(|offered| offered.iter().find(|(known, _)| known == name).map(|(_, handler)| Arc::clone(handler)));
    let handler = handler.ok_or_else(|| format!("{name} is not offered in this session"))?;
    handler(input, workspace)
}

pub fn forget(client: u64) {
    lock().remove(&client);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_client_gets_the_tools_its_host_offered_until_it_is_released() {
        let client = crate::tools::clients::fresh();
        assert!(tools(client).is_empty());
        let handler: Handler = Arc::new(|input, workspace| {
            Ok(json!(format!("{} in {}", input["directory"].as_str().unwrap_or_default(), workspace.cwd.display())))
        });
        offer(client, "AddRepository", handler);
        let offered = tools(client);
        assert_eq!(offered.iter().map(|tool| tool.spec().name).collect::<Vec<_>>(), ["AddRepository"]);
        let mut workspace = Workspace::new("/repo");
        workspace.credentials.client = client;
        let outcome = offered[0].run(&json!({ "directory": "api" }), &workspace);
        assert_eq!((outcome.content.as_str(), outcome.is_error), ("api in /repo", false));
        assert!(tools(crate::tools::clients::fresh()).is_empty());
        crate::tools::clients::release(client);
        assert!(tools(client).is_empty());
        let gone = offered[0].run(&json!({ "directory": "api" }), &workspace);
        assert!(gone.is_error && gone.content.contains("not offered"), "{gone:?}");
    }
}
