use std::sync::Arc;

use connectors::memory::ConnectorMemory;
use serde_json::Value;
use store::Store;

#[derive(Debug)]
pub struct StoredMemory(pub Arc<Store>);

impl ConnectorMemory for StoredMemory {
    fn get(&self, connection: &str, name: &str) -> Result<Value, String> {
        match self.0.connector_memory(connection, name).map_err(|error| error.to_string())? {
            Some(text) => serde_json::from_str(&text).map_err(|error| error.to_string()),
            None => Ok(Value::Null),
        }
    }

    fn set(&self, connection: &str, name: &str, value: &Value) -> Result<(), String> {
        let text = serde_json::to_string(value).map_err(|error| error.to_string())?;
        self.0.set_connector_memory(connection, name, &text).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_connector_value_comes_back_as_the_json_it_was() {
        let memory = StoredMemory(Arc::new(Store::in_memory(0).expect("store")));
        assert_eq!(memory.get("a", "ledger"), Ok(Value::Null));
        assert_eq!(memory.set("a", "ledger", &json!({"n": [1, "x"]})), Ok(()));
        assert_eq!(memory.get("a", "ledger"), Ok(json!({"n": [1, "x"]})));
        assert_eq!(memory.get("b", "ledger"), Ok(Value::Null));
    }
}
