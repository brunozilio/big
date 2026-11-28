use std::sync::{Arc, Mutex, PoisonError, RwLock};

use serde_json::Value;

use super::{Engine, Rules};

const SHIPPED: &str = include_str!("../../../../../content/safety/redaction.json");

static DECLARED: Mutex<Vec<(String, Rules)>> = Mutex::new(Vec::new());
static CURRENT: RwLock<Option<Arc<Engine>>> = RwLock::new(None);

pub fn shipped() -> Rules {
    serde_json::from_str::<Value>(SHIPPED).ok().and_then(|value| Rules::read(&value).ok()).unwrap_or_default()
}

fn built(declared: &[(String, Rules)]) -> (Engine, Vec<String>) {
    let mut rules = shipped();
    for (_, more) in declared {
        rules.merge(more);
    }
    Engine::compiled(rules)
}

pub fn declare(source: &str, rules: Rules) -> Vec<String> {
    let mut declared = DECLARED.lock().unwrap_or_else(PoisonError::into_inner);
    match declared.iter_mut().find(|(known, _)| known == source) {
        Some((_, kept)) => *kept = rules,
        None => declared.push((source.to_owned(), rules)),
    }
    let (engine, problems) = built(&declared);
    *CURRENT.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(engine));
    problems
}

pub fn current() -> Arc<Engine> {
    if let Some(engine) = CURRENT.read().unwrap_or_else(PoisonError::into_inner).as_ref() {
        return Arc::clone(engine);
    }
    let (engine, _) = built(&DECLARED.lock().unwrap_or_else(PoisonError::into_inner));
    let mut current = CURRENT.write().unwrap_or_else(PoisonError::into_inner);
    Arc::clone(current.get_or_insert_with(|| Arc::new(engine)))
}
