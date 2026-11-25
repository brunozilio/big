use std::sync::Arc;

use async_trait::async_trait;
use domain::Language;
use jiff::Timestamp;
use serde_json::{Value, json};

use super::bridge::{Script, invoke};
use super::host::{Local, Online};
use crate::descriptor::TokenField;
use crate::error::ActionError;
use crate::http::Http;
use crate::oauth::{Authorization, Grant, Loopback, LoopbackSignIn};

#[derive(Debug, Clone)]
pub struct ScriptedSignIn {
    script: Script,
    key: String,
    state: Value,
    name: String,
    port: Option<u16>,
}

fn parsed<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ActionError> {
    serde_json::from_value(value).map_err(|error| ActionError::permanent(format!("{what}: {error}")))
}

pub fn redirect() -> String {
    brand::connectors::OAUTH_REDIRECT.to_owned()
}

impl ScriptedSignIn {
    pub fn new(script: Script, key: &str, state: Value) -> ScriptedSignIn {
        let input = json!({ "state": state });
        let local: Arc<dyn script::Host> = Arc::new(Local);
        let named = script.invoke_here("signIn.name", &input, Arc::clone(&local)).ok();
        let name = named.and_then(|name| name.as_str().map(str::to_owned)).unwrap_or_else(|| key.to_owned());
        let port = script.invoke_here("signIn.port", &input, local).ok().and_then(|port| port.as_u64());
        let port = port.and_then(|port| u16::try_from(port).ok());
        ScriptedSignIn { script, key: key.to_owned(), state, name, port }
    }

    fn input(&self, extra: Value) -> Value {
        let mut input = json!({ "state": self.state, "key": self.key });
        for (name, value) in extra.as_object().into_iter().flatten() {
            input[name] = value.clone();
        }
        input
    }

    async fn online(&self, http: &dyn Http, name: &str, extra: Value) -> Result<Value, ActionError> {
        let online = Online { http, redirect: redirect() };
        invoke(&self.script, Arc::new(Local), &online, name, self.input(extra)).await
    }
}

#[async_trait]
impl LoopbackSignIn for ScriptedSignIn {
    fn name(&self) -> &str {
        &self.name
    }

    fn callback_port(&self) -> Option<u16> {
        self.port
    }

    fn fields(&self, language: Language) -> Vec<TokenField> {
        let input = self.input(json!({ "language": language.code() }));
        let listed = self.script.invoke_here("signIn.fields", &input, Arc::new(Local)).unwrap_or_default();
        serde_json::from_value(listed).unwrap_or_default()
    }

    fn for_fields(&self, fields: &Value, language: Language) -> Result<Option<Loopback>, String> {
        let input = self.input(json!({ "fields": fields, "language": language.code() }));
        let chosen =
            self.script.invoke_here("signIn.forFields", &input, Arc::new(Local)).map_err(|error| error.message)?;
        if chosen.is_null() {
            return Ok(None);
        }
        let state = chosen.get("state").cloned().unwrap_or(chosen);
        Ok(Some(Loopback::new(ScriptedSignIn::new(self.script.clone(), &self.key, state))))
    }

    async fn without_sign_in(&self, http: &dyn Http) -> Result<Option<Grant>, ActionError> {
        let found = self.online(http, "signIn.withoutSignIn", json!({})).await?;
        if found.is_null() { Ok(None) } else { parsed(found, "grant").map(Some) }
    }

    async fn begin(&self, http: &dyn Http, port: u16) -> Result<Authorization, ActionError> {
        parsed(self.online(http, "signIn.begin", json!({ "port": port })).await?, "authorization")
    }

    async fn exchange(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        code: &str,
        now: Timestamp,
    ) -> Result<Grant, ActionError> {
        let extra = json!({ "authorization": authorization, "code": code, "now": now.as_second() });
        parsed(self.online(http, "signIn.exchange", extra).await?, "grant")
    }

    async fn refresh(&self, http: &dyn Http, grant: &Grant, now: Timestamp) -> Result<Grant, ActionError> {
        parsed(self.online(http, "signIn.refresh", json!({ "grant": grant, "now": now.as_second() })).await?, "grant")
    }
}
