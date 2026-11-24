use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use platform::sync::Locked;

use agent_engine::model::http::Http;
use domain::SecretId;
use domain::credentials::{McpSignIns, McpTokenSource};
use domain::mcp_oauth::{self, McpTokens};
use serde_json::Value;

use crate::secrets::SecretStore;
use llm_providers::oauth::challenge;

const SLOT: SecretId = SecretId::from_uuid(uuid::Uuid::from_u128(0x6d63_705f_6f61_7574_6800_0000_0000_0001));
const FORM: &str = "application/x-www-form-urlencoded";

#[derive(Clone)]
pub struct McpAuth {
    secrets: Arc<dyn SecretStore>,
    http: Arc<dyn Http>,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    lock: Arc<Mutex<()>>,
}

impl std::fmt::Debug for McpAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("McpAuth")
    }
}

impl McpAuth {
    pub fn new(secrets: Arc<dyn SecretStore>, http: Arc<dyn Http>) -> Self {
        let clock = Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(i64::MAX))
        });
        Self { secrets, http, clock, lock: Arc::default() }
    }

    pub fn with_secrets(mut self, secrets: Arc<dyn SecretStore>) -> Self {
        self.secrets = secrets;
        self
    }

    #[cfg(test)]
    fn at(mut self, now: i64) -> Self {
        self.clock = Arc::new(move || now);
        self
    }

    pub fn for_run(&self) -> McpSignIns {
        McpSignIns(Arc::new(RunTokens { auth: self.clone() }))
    }

    pub fn token(&self, server: &str) -> Option<String> {
        let tokens = {
            let _guard = self.lock.locked();
            self.load(server)?
        };
        let now = (self.clock)();
        if tokens.fresh(now) {
            return Some(tokens.access_token);
        }
        let _held = self
            .secrets
            .hold_for_refresh(refresh_slot(server))
            .inspect_err(|error| tracing::warn!(%error, server, "an MCP sign-in could not be refreshed"))
            .ok()?;
        let tokens = {
            let _guard = self.lock.locked();
            self.load(server)?
        };
        if tokens.fresh(now) {
            return Some(tokens.access_token);
        }
        let refresh = tokens.refresh_token.clone()?;
        let mut form = vec![
            ("grant_type", "refresh_token".to_owned()),
            ("refresh_token", refresh),
            ("client_id", tokens.client_id.clone()),
            ("resource", tokens.resource.clone()),
        ];
        if let Some(secret) = &tokens.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        let refreshed = self
            .post_form(&tokens.token_endpoint, &form)
            .and_then(|answer| tokens.answered(&answer, now))
            .inspect_err(|error| tracing::warn!(%error, server, "an MCP sign-in could not be refreshed"))
            .ok()?;
        let _guard = self.lock.locked();
        match self.load(server) {
            Some(kept) if kept.access_token != tokens.access_token && kept.fresh(now) => Some(kept.access_token),
            None => None,
            Some(_) => {
                if let Err(error) = self.save(server, Some(&refreshed)) {
                    tracing::warn!(%error, server, "a refreshed MCP sign-in could not be kept");
                }
                Some(refreshed.access_token)
            }
        }
    }

    fn post_form(&self, url: &str, form: &[(&str, String)]) -> Result<Value, String> {
        let headers =
            [("content-type".to_owned(), FORM.to_owned()), ("accept".to_owned(), "application/json".to_owned())];
        self.read(self.http.post(url, &headers, &mcp_oauth::form(form)))
    }

    fn read(&self, response: Result<agent_engine::model::http::Response, String>) -> Result<Value, String> {
        let response = response?;
        let status = response.status;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::Read::take(response.body, 1 << 20), &mut text)
            .map_err(|error| error.to_string())?;
        let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if (200..300).contains(&status) {
            return Ok(value);
        }
        let said = value["error_description"].as_str().or(value["error"].as_str()).unwrap_or_default();
        Err(format!("HTTP {status} {said}").trim_end().to_owned())
    }

    fn load(&self, server: &str) -> Option<McpTokens> {
        let kept = self.secrets.get(SLOT)?;
        serde_json::from_str(kept.get(server)?).ok()
    }

    fn save(&self, server: &str, tokens: Option<&McpTokens>) -> Result<(), String> {
        let json = tokens.map(serde_json::to_string).transpose().map_err(|e| e.to_string())?;
        self.secrets.update(SLOT, &mut |kept| match &json {
            Some(json) => {
                kept.insert(server.to_owned(), json.clone());
            }
            None => {
                kept.remove(server);
            }
        })
    }
}

fn refresh_slot(server: &str) -> SecretId {
    let digest = URL_SAFE_NO_PAD.decode(challenge(server)).unwrap_or_default();
    let mut bytes = [0u8; 16];
    for (slot, byte) in bytes.iter_mut().zip(digest) {
        *slot = byte;
    }
    SecretId::from_uuid(uuid::Uuid::from_bytes(bytes))
}

struct RunTokens {
    auth: McpAuth,
}

impl McpTokenSource for RunTokens {
    fn token(&self, server_url: &str) -> Option<String> {
        self.auth.token(server_url)
    }
}

#[cfg(test)]
mod tests;
