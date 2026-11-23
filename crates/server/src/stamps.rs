use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use serde_json::{Value, json};

use crate::marks::{iso, now_ms};

#[derive(Debug, Default)]
pub struct Stamps {
    known: Mutex<HashMap<String, (String, i64)>>,
}

impl Stamps {
    pub fn stamp(&self, mut item: Value) -> (Value, bool) {
        let id = item["id"].as_str().unwrap_or_default().to_owned();
        item["updatedAt"] = Value::Null;
        let signature = item.to_string();
        let mut known = self.known.lock().unwrap_or_else(PoisonError::into_inner);
        let (changed, at) = match known.get(&id) {
            Some((seen, at)) if *seen == signature => (false, *at),
            Some((_, at)) => (true, now_ms().max(at + 1)),
            None => (true, now_ms()),
        };
        known.insert(id, (signature, at));
        item["updatedAt"] = json!(iso(at));
        (item, changed)
    }

    pub fn forget(&self, id: &str) -> bool {
        self.known.lock().unwrap_or_else(PoisonError::into_inner).remove(id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_moves_only_when_the_item_changes() {
        let stamps = Stamps::default();
        let (first, changed) = stamps.stamp(json!({ "id": "a", "x": 1 }));
        assert!(changed);
        let (same, changed) = stamps.stamp(json!({ "id": "a", "x": 1 }));
        assert!(!changed);
        assert_eq!(first["updatedAt"], same["updatedAt"]);
        let (moved, changed) = stamps.stamp(json!({ "id": "a", "x": 2 }));
        assert!(changed);
        assert_ne!(moved["updatedAt"], first["updatedAt"]);
        assert!(stamps.forget("a"));
    }
}
