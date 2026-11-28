use domain::connection::{Connection, ConnectionId, ProviderKey};
use serde_json::{Value, json};

use crate::error::ActionError;
use crate::http::Http;
use crate::hub::Connectors;
use crate::limiter::Metered;
use crate::provider::{Credentials, Provider};

pub const CAPABILITY: &str = "audit";
pub const DELIVER: &str = "audit.deliver";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ack {
    pub ack: usize,
    pub rejected: usize,
}

impl Ack {
    pub fn taken(&self) -> usize {
        self.ack + self.rejected
    }
}

fn count(answer: &Value, name: &str) -> Result<usize, ActionError> {
    match answer.get(name) {
        None | Some(Value::Null) => Ok(0),
        Some(value) => value
            .as_u64()
            .and_then(|count| usize::try_from(count).ok())
            .ok_or_else(|| ActionError::decode(format!("`{name}` is not a count: {value}"))),
    }
}

pub fn ack_of(answer: &Value, offered: usize) -> Result<Ack, ActionError> {
    if answer.get("ack").is_none() {
        return Err(ActionError::decode(format!("a sink answers {{ ack }}, got {answer}")));
    }
    let ack = Ack { ack: count(answer, "ack")?, rejected: count(answer, "rejected")? };
    if ack.taken() > offered {
        return Err(ActionError::permanent(format!("the sink took {} of {offered} events", ack.taken())));
    }
    Ok(ack)
}

pub async fn delivered(
    provider: &dyn Provider,
    http: &dyn Http,
    creds: &Credentials,
    events: &[Value],
    idempotency_key: &str,
) -> Result<Ack, ActionError> {
    let answer = provider.call(http, creds, DELIVER, &json!({ "events": events }), idempotency_key).await?;
    ack_of(&answer, events.len())
}

impl Connectors {
    pub async fn deliver_audit(
        &self,
        key: &ProviderKey,
        bound: Option<(&Connection, &Value)>,
        events: &[Value],
        idempotency_key: &str,
    ) -> Result<Ack, ActionError> {
        if let Some((connection, settings)) = bound {
            let args = json!({ "events": events });
            let answer = self.call(connection, settings, DELIVER, &args, idempotency_key).await?;
            return ack_of(&answer, events.len());
        }
        let entry = self.entry(key)?;
        let id = ConnectionId::from(key.as_str());
        let creds = Credentials::new(&id, "", json!({}));
        let metered = Metered { inner: self.http(), limiter: self.limiter(), connection: &id };
        delivered(entry.provider.as_ref(), &metered, &creds, events, idempotency_key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ack_is_a_prefix_of_what_was_offered() {
        assert_eq!(ack_of(&json!({ "ack": 2 }), 3).ok(), Some(Ack { ack: 2, rejected: 0 }));
        assert_eq!(ack_of(&json!({ "ack": 1, "rejected": 2 }), 3).ok(), Some(Ack { ack: 1, rejected: 2 }));
        assert!(ack_of(&json!({ "ack": 4 }), 3).is_err());
        assert!(ack_of(&json!({ "ack": -1 }), 3).is_err());
        assert!(ack_of(&json!({}), 3).is_err());
        assert!(ack_of(&json!({ "ack": 0 }), 3).is_ok_and(|ack| ack.taken() == 0));
    }
}
