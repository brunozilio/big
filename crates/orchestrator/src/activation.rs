use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use crate::bundle::{Bundle, ENTRY, bundle};
use crate::resources::Resources;

struct NoHost;

impl script::Host for NoHost {
    fn call(&self, function: &str, _arguments: Value) -> Result<Value, String> {
        Err(format!("a workflow reaches the world only through ctx, yet it called {function}"))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    bundle: Bundle,
    pub target: Value,
    pub described: Value,
}

impl Script {
    pub fn load(resources: &Resources, workflow: &str) -> Result<Script, String> {
        let found = resources.workflow(workflow).ok_or_else(|| format!("there is no workflow named {workflow}"))?;
        Script::built(bundle(resources, &found.path)?, workflow)
    }

    fn built(bundle: Bundle, workflow: &str) -> Result<Script, String> {
        let mut script = Script { bundle, target: Value::Null, described: Value::Null };
        script.described = script.call("host.describe", &[])?;
        let names: Vec<&str> = script.described["workflows"]
            .as_array()
            .map(|all| all.iter().filter_map(|shape| shape["name"].as_str()).collect())
            .unwrap_or_default();
        let chosen = names.iter().find(|name| **name == workflow).or_else(|| names.last());
        let chosen = chosen.ok_or_else(|| format!("the module of {workflow} registers no workflow"))?;
        script.target = json!({ "workflow": chosen });
        Ok(script)
    }

    pub fn activate(&self, activation: &Value) -> Result<Value, String> {
        self.call("host.activate", std::slice::from_ref(activation))
    }

    pub fn view(&self, request: &Value) -> Result<Value, String> {
        self.call("host.view", std::slice::from_ref(request))
    }

    fn call(&self, export: &str, arguments: &[Value]) -> Result<Value, String> {
        let host: Arc<dyn script::Host> = Arc::new(NoHost);
        let stop = Arc::new(AtomicBool::new(false));
        let limit = script::Limit::Busy(script::MOST_BUSY);
        script::run_modules(&self.bundle.modules, ENTRY, export, arguments, host, stop, limit)
    }
}
