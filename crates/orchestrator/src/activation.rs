use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};

use crate::bundle::{Bundle, ENTRY, bundle};
use crate::resources::Resources;

struct NoHost;

impl script::Host for NoHost {
    fn call(&self, function: &str, _arguments: Value) -> Result<Value, String> {
        Err(format!("a workflow reaches the world only through ctx, yet it called {function}"))
    }
}

pub const VIEWS_KEPT: usize = 64;

#[derive(Debug)]
pub struct Script {
    bundle: Bundle,
    pub target: Value,
    pub described: Value,
    views: Mutex<Vec<(String, Value)>>,
}

impl Script {
    pub fn load(resources: &Resources, workflow: &str) -> Result<Script, String> {
        let found = resources.workflow(workflow).ok_or_else(|| format!("there is no workflow named {workflow}"))?;
        Script::built(bundle(resources, &found.path)?, workflow)
    }

    fn built(bundle: Bundle, workflow: &str) -> Result<Script, String> {
        let mut script = Script { bundle, target: Value::Null, described: Value::Null, views: Mutex::default() };
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
        let key = request.to_string();
        let kept =
            |views: &[(String, Value)]| views.iter().find(|(known, _)| *known == key).map(|(_, view)| view.clone());
        if let Some(found) = kept(&self.views.lock().unwrap_or_else(PoisonError::into_inner)) {
            return Ok(found);
        }
        let view = self.call("host.view", std::slice::from_ref(request))?;
        let mut views = self.views.lock().unwrap_or_else(PoisonError::into_inner);
        if kept(&views).is_none() {
            if views.len() >= VIEWS_KEPT {
                views.remove(0);
            }
            views.push((key, view.clone()));
        }
        Ok(view)
    }

    #[cfg(test)]
    pub fn views_kept(&self) -> usize {
        self.views.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    fn call(&self, export: &str, arguments: &[Value]) -> Result<Value, String> {
        let host: Arc<dyn script::Host> = Arc::new(NoHost);
        let stop = Arc::new(AtomicBool::new(false));
        let limit = script::Limit::Busy(script::MOST_BUSY);
        script::run_modules(&self.bundle.modules, ENTRY, export, arguments, host, stop, limit)
    }
}
