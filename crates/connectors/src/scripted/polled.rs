use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use super::bridge::{Answers, Script, invoke};
use super::host::{Kept, Online};
use super::sessions::McpSessions;
use super::sign_in::redirect;
use crate::error::ActionError;
use crate::http::Http;
use crate::provider::{Credentials, EventBatch, EventSource, ExternalEvent};

#[derive(Debug)]
pub(super) struct ScriptedEvents {
    pub(super) script: Script,
    pub(super) answers: Arc<dyn Answers>,
    pub(super) sessions: Arc<McpSessions>,
}

#[async_trait]
impl EventSource for ScriptedEvents {
    async fn poll(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        cursor: Option<&str>,
        watch: &[String],
    ) -> Result<EventBatch, ActionError> {
        let input = json!({ "creds": super::creds_of(creds), "cursor": cursor, "watch": watch });
        let online = Online::pooled(http, redirect(), Arc::clone(&self.sessions), &input);
        let answers = Kept::of(&self.answers, &input);
        let batch = invoke(&self.script, answers, &online, "poll", input).await?;
        let mut events = Vec::new();
        for mut event in batch["events"].as_array().cloned().unwrap_or_default() {
            event["connection"] = json!(creds.connection);
            events.push(serde_json::from_value::<ExternalEvent>(event).map_err(ActionError::decode)?);
        }
        Ok(EventBatch { events, cursor: batch["cursor"].as_str().unwrap_or_default().to_owned() })
    }
}
