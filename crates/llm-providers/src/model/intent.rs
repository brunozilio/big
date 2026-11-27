use serde_json::{Value, json};

use super::TurnRequest;
use super::conversation_key::conversation_key;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cache {
    Long,
    #[default]
    Short,
    None,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    #[default]
    Now,
    Later,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Intent {
    #[serde(default)]
    pub cache: Cache,
    #[serde(default)]
    pub priority: Priority,
}

impl Intent {
    pub const SESSION: Self = Self { cache: Cache::Long, priority: Priority::Now };
    pub const ONCE: Self = Self { cache: Cache::None, priority: Priority::Now };
    pub const LATER: Self = Self { cache: Cache::None, priority: Priority::Later };
}

pub fn discounted(cost: Option<f64>, intent: Intent, discount: Option<f64>) -> Option<f64> {
    match (intent.priority, discount) {
        (Priority::Later, Some(discount)) => cost.map(|cost| cost * (1.0 - discount.clamp(0.0, 1.0))),
        _ => cost,
    }
}

const FIELD: &str = "intent";

pub fn put(request: &TurnRequest, body: &mut Value) {
    let intent = request.intent;
    let key = conversation_key(request).to_string();
    body[FIELD] = json!({ "cache": intent.cache, "priority": intent.priority, "key": key });
}

pub fn sent(request: &TurnRequest, mut body: Value) -> String {
    put(request, &mut body);
    body.to_string()
}

pub fn take(body: &mut Value) -> Value {
    body.as_object_mut().and_then(|object| object.remove(FIELD)).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Message;
    use domain::Effort;

    fn request(intent: Intent) -> TurnRequest {
        TurnRequest {
            model: "m".into(),
            system: "s".into(),
            messages: vec![Message::user("hi")],
            tools: Vec::new(),
            effort: Effort::Low,
            max_tokens: 10,
            intent,
        }
    }

    #[test]
    fn the_intent_rides_the_body_until_it_is_taken() {
        let mut body = json!({"model": "m"});
        put(&request(Intent::SESSION), &mut body);
        assert_eq!(body["intent"]["cache"], "long");
        assert_eq!(body["intent"]["key"].as_str().map(str::len), Some(36));
        let taken = take(&mut body);
        assert_eq!(taken["cache"], "long");
        assert_eq!(body, json!({"model": "m"}));
    }

    #[test]
    fn a_one_off_call_asks_for_no_cache_and_others_for_the_short_one() {
        assert_eq!(serde_json::to_value(Intent::ONCE).ok(), Some(json!({"cache": "none", "priority": "now"})));
        assert_eq!(Intent::default().cache, Cache::Short);
        assert_eq!(take(&mut json!({})), Value::Null);
    }

    #[test]
    fn a_call_that_can_wait_says_so_and_is_charged_the_declared_discount() {
        let mut body = json!({});
        put(&request(Intent::LATER), &mut body);
        assert_eq!(body["intent"]["priority"], "later");
        assert_eq!(discounted(Some(2.0), Intent::LATER, Some(0.5)), Some(1.0));
        assert_eq!(discounted(Some(2.0), Intent::LATER, None), Some(2.0), "no low-priority mode: the full price");
        assert_eq!(discounted(Some(2.0), Intent::ONCE, Some(0.5)), Some(2.0), "an urgent call pays in full");
    }
}
