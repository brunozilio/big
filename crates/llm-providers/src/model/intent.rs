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
pub struct Intent {
    #[serde(default)]
    pub cache: Cache,
}

impl Intent {
    pub const SESSION: Self = Self { cache: Cache::Long };
    pub const ONCE: Self = Self { cache: Cache::None };
}

const FIELD: &str = "intent";

pub fn put(request: &TurnRequest, body: &mut Value) {
    body[FIELD] = json!({ "cache": request.intent.cache, "key": conversation_key(request).to_string() });
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
        assert_eq!(serde_json::to_value(Intent::ONCE).ok(), Some(json!({"cache": "none"})));
        assert_eq!(Intent::default().cache, Cache::Short);
        assert_eq!(take(&mut json!({})), Value::Null);
    }
}
