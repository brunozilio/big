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
use crate::oauth::{Authorization, DeviceSignIn, Grant, Loopback, LoopbackSignIn, Polled, Proceeded, Started};

#[derive(Debug, Clone)]
pub struct ScriptedSignIn {
    script: Script,
    key: String,
    using: String,
    state: Value,
    name: String,
    port: Option<u16>,
}

pub const SIGN_IN: &str = "signIn";

fn parsed<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ActionError> {
    serde_json::from_value(value).map_err(|error| ActionError::permanent(format!("{what}: {error}")))
}

pub fn redirect() -> String {
    brand::connectors::OAUTH_REDIRECT.to_owned()
}

impl ScriptedSignIn {
    pub fn new(script: Script, key: &str, using: &str, state: Value) -> ScriptedSignIn {
        let mut made = ScriptedSignIn {
            script,
            key: key.to_owned(),
            using: using.to_owned(),
            state,
            name: key.to_owned(),
            port: None,
        };
        let input = made.input(json!({}));
        let local: Arc<dyn script::Host> = Arc::new(Local);
        let named = made.script.invoke_here(&made.named("name"), &input, Arc::clone(&local)).ok();
        if let Some(name) = named.and_then(|name| name.as_str().map(str::to_owned)) {
            made.name = name;
        }
        let port = made.script.invoke_here(&made.named("port"), &input, local).ok().and_then(|port| port.as_u64());
        made.port = port.and_then(|port| u16::try_from(port).ok());
        made
    }

    fn named(&self, step: &str) -> String {
        format!("{}.{step}", self.using)
    }

    fn input(&self, extra: Value) -> Value {
        let mut input = json!({ "state": self.state, "key": self.key });
        for (name, value) in extra.as_object().into_iter().flatten() {
            input[name] = value.clone();
        }
        input
    }

    async fn online(&self, http: &dyn Http, name: &str, extra: Value) -> Result<Value, ActionError> {
        let online = Online::new(http, redirect());
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
        let listed = self.script.invoke_here(&self.named("fields"), &input, Arc::new(Local)).unwrap_or_default();
        serde_json::from_value(listed).unwrap_or_default()
    }

    fn for_fields(&self, fields: &Value, language: Language) -> Result<Option<Loopback>, String> {
        let input = self.input(json!({ "fields": fields, "language": language.code() }));
        let chosen = self
            .script
            .invoke_here(&self.named("forFields"), &input, Arc::new(Local))
            .map_err(|error| error.message)?;
        if chosen.is_null() {
            return Ok(None);
        }
        let state = chosen.get("state").cloned().unwrap_or(chosen);
        Ok(Some(Loopback::new(ScriptedSignIn::new(self.script.clone(), &self.key, &self.using, state))))
    }

    async fn without_sign_in(&self, http: &dyn Http) -> Result<Option<Grant>, ActionError> {
        let found = self.online(http, &self.named("withoutSignIn"), json!({})).await?;
        if found.is_null() { Ok(None) } else { parsed(found, "grant").map(Some) }
    }

    async fn begin(&self, http: &dyn Http, port: u16) -> Result<Authorization, ActionError> {
        parsed(self.online(http, &self.named("begin"), json!({ "port": port })).await?, "authorization")
    }

    async fn exchange(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        code: &str,
        now: Timestamp,
    ) -> Result<Grant, ActionError> {
        let extra = json!({ "authorization": authorization, "code": code, "now": now.as_second() });
        parsed(self.online(http, &self.named("exchange"), extra).await?, "grant")
    }

    async fn proceed(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        query: &Value,
        now: Timestamp,
    ) -> Result<Proceeded, ActionError> {
        let extra = json!({ "authorization": authorization, "query": query, "now": now.as_second() });
        let answer = self.online(http, &self.named("proceed"), extra).await?;
        if !answer["grant"].is_null() {
            return parsed(answer["grant"].clone(), "grant").map(Proceeded::Granted);
        }
        parsed(answer["authorization"].clone(), "authorization").map(Proceeded::Redirect)
    }

    async fn refresh(&self, http: &dyn Http, grant: &Grant, now: Timestamp) -> Result<Grant, ActionError> {
        parsed(
            self.online(http, &self.named("refresh"), json!({ "grant": grant, "now": now.as_second() })).await?,
            "grant",
        )
    }
}

fn polled(answer: Value) -> Result<Polled, ActionError> {
    match answer["state"].as_str().unwrap_or_default() {
        "pending" => Ok(Polled::Pending),
        "slow_down" => Ok(Polled::SlowDown { interval: answer["interval"].as_i64() }),
        "expired" => Ok(Polled::Expired),
        "denied" => Ok(Polled::Denied),
        "authorized" => parsed(answer["grant"].clone(), "grant").map(Polled::Authorized),
        other => Err(ActionError::permanent(format!("the sign-in step answered an unknown state `{other}`"))),
    }
}

#[async_trait]
impl DeviceSignIn for ScriptedSignIn {
    fn name(&self) -> &str {
        &self.name
    }

    fn fields(&self, language: Language) -> Vec<TokenField> {
        LoopbackSignIn::fields(self, language)
    }

    async fn start(&self, http: &dyn Http, fields: &Value, language: Language) -> Result<Started, ActionError> {
        let extra = json!({ "fields": fields, "language": language.code() });
        parsed(self.online(http, &self.named("start"), extra).await?, "start")
    }

    async fn poll(&self, http: &dyn Http, started: &Started, now: Timestamp) -> Result<Polled, ActionError> {
        let extra = json!({ "started": started, "now": now.as_second() });
        polled(self.online(http, &self.named("poll"), extra).await?)
    }

    async fn refresh(&self, http: &dyn Http, grant: &Grant, now: Timestamp) -> Result<Grant, ActionError> {
        LoopbackSignIn::refresh(self, http, grant, now).await
    }
}
