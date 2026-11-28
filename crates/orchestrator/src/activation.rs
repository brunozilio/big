use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};

use domain::workflow::Output;
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
    pub output: Option<Output>,
    views: Mutex<Vec<(String, Value)>>,
}

fn versioned(described: &mut Value, workflow: &str) -> Result<(), String> {
    use content::schema::{Contract, read};
    let at = format!("workflows/{workflow}");
    for shape in described["workflows"].as_array_mut().into_iter().flatten() {
        read(Contract::Workflow, shape, &at).map_err(|error| error.to_string())?;
        for trigger in shape["triggers"].as_array_mut().into_iter().flatten() {
            read(Contract::Trigger, trigger, &at).map_err(|error| error.to_string())?;
        }
        packages(&mut shape["steps"], &at)?;
        Output::read(&shape["output"]).map_err(|error| format!("{at}: {error}"))?;
    }
    Ok(())
}

fn packages(steps: &mut Value, at: &str) -> Result<(), String> {
    use content::schema::{Contract, FIELD, read};
    for call in steps.as_array_mut().into_iter().flatten() {
        if call["options"].get(FIELD).is_some() {
            read(Contract::Package, &mut call["options"], at).map_err(|error| error.to_string())?;
        }
        packages(&mut call["steps"], at)?;
    }
    Ok(())
}

impl Script {
    pub fn load(resources: &Resources, workflow: &str) -> Result<Script, String> {
        let found = resources.workflow(workflow).ok_or_else(|| format!("there is no workflow named {workflow}"))?;
        Script::built(bundle(resources, &found.path)?, workflow)
    }

    fn built(bundle: Bundle, workflow: &str) -> Result<Script, String> {
        let mut script =
            Script { bundle, target: Value::Null, described: Value::Null, output: None, views: Mutex::default() };
        script.described = script.call("host.describe", &[])?;
        versioned(&mut script.described, workflow)?;
        let names: Vec<&str> = script.described["workflows"]
            .as_array()
            .map(|all| all.iter().filter_map(|shape| shape["name"].as_str()).collect())
            .unwrap_or_default();
        let chosen = names.iter().find(|name| **name == workflow).or_else(|| names.last());
        let chosen = chosen.ok_or_else(|| format!("the module of {workflow} registers no workflow"))?;
        script.target = json!({ "workflow": chosen });
        let shapes = script.described["workflows"].as_array().into_iter().flatten();
        let shape = shapes.into_iter().find(|shape| shape["name"] == script.target["workflow"]);
        script.output = shape.and_then(|shape| Output::read(&shape["output"]).ok().flatten());
        Ok(script)
    }

    pub fn activate(&self, activation: &Value) -> Result<Value, String> {
        self.call("host.activate", std::slice::from_ref(activation))
    }

    pub fn branch(&self, asked: &Value) -> Result<Value, String> {
        self.call("host.branch", &[self.target.clone(), asked.clone()])
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
