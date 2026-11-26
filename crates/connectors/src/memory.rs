use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use serde_json::Value;

pub trait ConnectorMemory: Send + Sync + std::fmt::Debug {
    fn get(&self, connection: &str, name: &str) -> Result<Value, String>;
    fn set(&self, connection: &str, name: &str, value: &Value) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct MemoryValues(Mutex<HashMap<(String, String), Value>>);

impl ConnectorMemory for MemoryValues {
    fn get(&self, connection: &str, name: &str) -> Result<Value, String> {
        let held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(held.get(&(connection.to_owned(), name.to_owned())).cloned().unwrap_or(Value::Null))
    }

    fn set(&self, connection: &str, name: &str, value: &Value) -> Result<(), String> {
        let mut held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        held.insert((connection.to_owned(), name.to_owned()), value.clone());
        Ok(())
    }
}

static MEMORY: OnceLock<Arc<dyn ConnectorMemory>> = OnceLock::new();

pub fn install(memory: Arc<dyn ConnectorMemory>) {
    let _ = MEMORY.set(memory);
}

pub fn installed() -> Option<&'static Arc<dyn ConnectorMemory>> {
    MEMORY.get()
}
