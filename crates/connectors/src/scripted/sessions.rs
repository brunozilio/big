use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::error::ActionError;
use crate::http::Http;
use crate::mcp::McpClient;

pub const IDLE: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub connection: String,
    pub url: String,
    pub token: String,
}

#[derive(Debug)]
struct Open {
    identity: Identity,
    client: Arc<McpClient>,
    used: Instant,
}

#[derive(Debug)]
pub enum Ask {
    Connect,
    Tools,
    Call(String, Value),
}

#[derive(Debug, Default)]
pub struct McpSessions {
    open: Mutex<Vec<Open>>,
}

async fn asked(client: &McpClient, http: &dyn Http, ask: &Ask) -> Result<Value, ActionError> {
    match ask {
        Ask::Connect => Ok(json!(true)),
        Ask::Tools => Ok(json!(client.tools(http).await?)),
        Ask::Call(tool, arguments) => client.call(http, tool, arguments.clone()).await,
    }
}

impl McpSessions {
    fn locked(&self) -> MutexGuard<'_, Vec<Open>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn kept(&self, identity: &Identity, now: Instant) -> Option<Arc<McpClient>> {
        let mut open = self.locked();
        open.retain(|session| now.saturating_duration_since(session.used) < IDLE && !session.client.stale());
        let found = open.iter_mut().find(|session| session.identity == *identity)?;
        found.used = now;
        Some(Arc::clone(&found.client))
    }

    fn keep(&self, identity: &Identity, client: &Arc<McpClient>) {
        let mut open = self.locked();
        open.retain(|session| session.identity != *identity);
        open.push(Open { identity: identity.clone(), client: Arc::clone(client), used: Instant::now() });
    }

    fn forget(&self, client: &Arc<McpClient>) {
        self.locked().retain(|session| !Arc::ptr_eq(&session.client, client));
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.locked().len()
    }

    async fn opened(&self, http: &dyn Http, identity: &Identity) -> Result<(Arc<McpClient>, bool), ActionError> {
        if let Some(client) = self.kept(identity, Instant::now()) {
            return Ok((client, true));
        }
        let client = Arc::new(McpClient::connect(http, &identity.url, &identity.token).await?);
        self.keep(identity, &client);
        Ok((client, false))
    }

    pub async fn ask(&self, http: &dyn Http, identity: &Identity, ask: &Ask) -> Result<Value, ActionError> {
        let (client, reused) = self.opened(http, identity).await?;
        let first = asked(&client, http, ask).await;
        if first.is_ok() || !client.stale() {
            return first;
        }
        self.forget(&client);
        if !reused {
            return first;
        }
        let (client, _) = self.opened(http, identity).await?;
        let again = asked(&client, http, ask).await;
        if client.stale() {
            self.forget(&client);
        }
        again
    }

    #[cfg(test)]
    pub fn age(&self, by: Duration) {
        for session in self.locked().iter_mut() {
            session.used = session.used.checked_sub(by).unwrap_or(session.used);
        }
    }
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
