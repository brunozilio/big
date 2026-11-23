use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use domain::connection::ConnectionId;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ConnectionState {
    Healthy,
    Failing { attempt: u32, reason: String },
    NeedsReconnect { reason: String },
}

#[derive(Debug, Default)]
pub struct Health {
    states: Mutex<BTreeMap<ConnectionId, ConnectionState>>,
}

impl Health {
    pub fn new() -> Health {
        Health::default()
    }

    fn held(&self) -> std::sync::MutexGuard<'_, BTreeMap<ConnectionId, ConnectionState>> {
        self.states.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn state(&self, connection: &ConnectionId) -> ConnectionState {
        self.held().get(connection).cloned().unwrap_or(ConnectionState::Healthy)
    }

    pub fn all(&self) -> Vec<(ConnectionId, ConnectionState)> {
        self.held().iter().map(|(id, state)| (id.clone(), state.clone())).collect()
    }

    pub fn needs_reconnect(&self, connection: &ConnectionId) -> bool {
        matches!(self.state(connection), ConnectionState::NeedsReconnect { .. })
    }

    pub fn revoked(&self, connection: &ConnectionId, reason: &str) -> bool {
        let next = ConnectionState::NeedsReconnect { reason: reason.to_owned() };
        let previous = self.held().insert(connection.clone(), next);
        !matches!(previous, Some(ConnectionState::NeedsReconnect { .. }))
    }

    pub fn failing(&self, connection: &ConnectionId, reason: &str) -> u32 {
        let mut states = self.held();
        let attempt = match states.get(connection) {
            Some(ConnectionState::Failing { attempt, .. }) => attempt.saturating_add(1),
            _ => 1,
        };
        states.insert(connection.clone(), ConnectionState::Failing { attempt, reason: reason.to_owned() });
        attempt
    }

    pub fn healthy(&self, connection: &ConnectionId) {
        self.held().remove(connection);
    }

    pub fn reconnected(&self, connection: &ConnectionId) {
        self.healthy(connection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_count_up_revocation_is_reported_once_and_success_clears_both() {
        let health = Health::new();
        let id = ConnectionId::from("c");
        assert_eq!(health.state(&id), ConnectionState::Healthy);
        assert_eq!(health.failing(&id, "x"), 1);
        assert_eq!(health.failing(&id, "y"), 2);
        assert!(health.revoked(&id, "gone"));
        assert!(!health.revoked(&id, "gone"));
        assert!(health.needs_reconnect(&id));
        assert_eq!(health.all().len(), 1);
        health.reconnected(&id);
        assert_eq!(health.state(&id), ConnectionState::Healthy);
        assert_eq!(health.failing(&id, "z"), 1);
    }

    #[test]
    fn states_serialize_for_the_interface() {
        let state = ConnectionState::NeedsReconnect { reason: "r".into() };
        assert_eq!(
            serde_json::to_value(state).ok(),
            Some(serde_json::json!({"state": "needs_reconnect", "reason": "r"}))
        );
    }
}
