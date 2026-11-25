mod bridge;
mod host;
mod sign_in;

use std::sync::Arc;

use async_trait::async_trait;
use domain::Language;
use domain::connection::{Capability, ProviderKey};
use serde::Deserialize;
use serde_json::{Map, Value, json};

pub use bridge::Script;
use bridge::{WRAPPER, invoke, wrapper};
pub use host::Local;
use host::Online;
pub use sign_in::ScriptedSignIn;
use sign_in::redirect;

use crate::descriptor::{ActionSpec, AlternateAuth, AuthSpec, Presentation, ProviderDescriptor, TokenSpec};
use crate::error::ActionError;
use crate::http::Http;
use crate::oauth::{DeviceApp, Loopback};
use crate::provider::{AccountOption, Credentials, EventBatch, EventSource, ExternalEvent, Provider};

pub const FOLDER: &str = "connectors/";
pub const ENTRY: &str = "index.ts";

#[derive(Debug, Deserialize)]
struct Declared {
    #[serde(default)]
    key: String,
    name: String,
    #[serde(default)]
    logo: String,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    actions: Vec<ActionSpec>,
    #[serde(default)]
    auth: Value,
    #[serde(default)]
    chooses: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Alternate {
    key: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    button: String,
    #[serde(default)]
    site: String,
    auth: Value,
}

#[derive(Debug)]
pub struct ScriptedProvider {
    key: String,
    script: Script,
    descriptor: ProviderDescriptor,
    chooses: Vec<String>,
    events: Option<ScriptedEvents>,
}

#[derive(Debug)]
struct ScriptedEvents {
    script: Script,
}

fn config_of(key: &str) -> Value {
    let pairs = |name: &str| -> Map<String, Value> {
        brand::section(name).iter().map(|(name, value)| ((*name).to_owned(), json!(value))).collect()
    };
    json!({ "config": pairs(key), "connectors": pairs("connectors"), "brand": { "NAME": brand::NAME, "COMMAND": brand::COMMAND } })
}

fn auth_of(script: &Script, key: &str, auth: &Value) -> Result<AuthSpec, String> {
    let kind = auth["kind"].as_str().unwrap_or_default();
    let parsed = |what: &str| format!("{key}: the {what} sign-in is not valid");
    match kind {
        "token" => serde_json::from_value::<TokenSpec>(auth.clone()).map(AuthSpec::Token).map_err(|_| parsed(kind)),
        "device" => serde_json::from_value::<DeviceApp>(auth.clone()).map(AuthSpec::Device).map_err(|_| parsed(kind)),
        "loopback" => {
            let state = auth.get("state").cloned().unwrap_or(json!({}));
            Ok(AuthSpec::Loopback(Loopback::new(ScriptedSignIn::new(script.clone(), key, state))))
        }
        other => Err(format!("{key}: no sign-in is called `{other}`")),
    }
}

fn logo_of(root: Option<&std::path::Path>, folder: &str, logo: &str) -> String {
    if logo.trim_start().starts_with('<') || logo.is_empty() {
        return logo.to_owned();
    }
    let path = format!("{folder}{logo}");
    content::bytes_in(root, &path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

impl ScriptedProvider {
    pub fn load(entry: &str) -> Result<ScriptedProvider, String> {
        ScriptedProvider::load_in(content::folder().as_deref(), entry)
    }

    pub fn load_in(root: Option<&std::path::Path>, entry: &str) -> Result<ScriptedProvider, String> {
        let mut modules = content::modules_in(root, entry)?;
        modules.push((WRAPPER.to_owned(), wrapper(entry)));
        let folder = entry.rsplit_once('/').map_or(String::new(), |(folder, _)| format!("{folder}/"));
        let named = folder.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_owned();
        let mut script = Script { modules: Arc::new(modules), data: config_of(&named) };
        let local: Arc<dyn script::Host> = Arc::new(Local);
        let declared = script.invoke_here("provider", &json!({}), local).map_err(|error| error.message)?;
        let declared: Declared = serde_json::from_value(declared).map_err(|error| format!("{entry}: {error}"))?;
        let key = if declared.key.is_empty() { named } else { declared.key.clone() };
        script.data = config_of(&key);
        let polls = script.kind("poll") == "function";
        let descriptor = ProviderDescriptor {
            key: ProviderKey::from(key.as_str()),
            name: declared.name,
            logo: logo_of(root, &folder, &declared.logo),
            capabilities: declared.capabilities.iter().map(|name| Capability::from(name.as_str())).collect(),
            auth: auth_of(&script, &key, &declared.auth)?,
            actions: declared.actions,
        };
        let events = polls.then(|| ScriptedEvents { script: script.clone() });
        Ok(ScriptedProvider { key, script, descriptor, chooses: declared.chooses, events })
    }

    fn local(&self, name: &str, language: Language) -> Value {
        let input = json!({ "language": language.code() });
        self.script.invoke_here(name, &input, Arc::new(Local)).unwrap_or_default()
    }

    async fn online(&self, http: &dyn Http, name: &str, input: Value) -> Result<Value, ActionError> {
        let online = Online { http, redirect: redirect() };
        invoke(&self.script, Arc::new(Local), &online, name, input).await
    }
}

fn creds_of(creds: &Credentials) -> Value {
    json!({ "connection": creds.connection, "token": creds.access_token, "settings": creds.settings })
}

pub fn discover() -> Vec<Arc<dyn Provider>> {
    let entries = content::listed(FOLDER);
    let entries = entries.iter().filter(|path| {
        path.strip_prefix(FOLDER)
            .and_then(|rest| rest.strip_suffix(ENTRY))
            .is_some_and(|name| name.matches('/').count() == 1 && name.ends_with('/'))
    });
    let mut found: Vec<Arc<dyn Provider>> = Vec::new();
    for entry in entries {
        match ScriptedProvider::load(entry) {
            Ok(provider) => found.push(Arc::new(provider)),
            Err(error) => tracing::error!(%error, entry, "a connector could not be loaded"),
        }
    }
    found
}

#[async_trait]
impl Provider for ScriptedProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        self.descriptor.clone()
    }

    async fn call(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        action: &str,
        args: &Value,
        idempotency_key: &str,
    ) -> Result<Value, ActionError> {
        let input = json!({ "action": action, "args": args, "creds": creds_of(creds), "idempotencyKey": idempotency_key });
        self.online(http, "call", input).await
    }

    async fn accounts(&self, http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        let found = self.online(http, "accounts", json!({ "creds": creds_of(creds) })).await?;
        serde_json::from_value(found).map_err(ActionError::decode)
    }

    fn events(&self) -> Option<&dyn EventSource> {
        self.events.as_ref().map(|events| events as &dyn EventSource)
    }

    fn presentation(&self, language: Language) -> Presentation {
        serde_json::from_value(self.local("presentation", language)).unwrap_or_default()
    }

    fn alternatives(&self, language: Language) -> Vec<AlternateAuth> {
        let listed: Vec<Alternate> = serde_json::from_value(self.local("alternatives", language)).unwrap_or_default();
        listed
            .into_iter()
            .filter_map(|alternate| {
                let auth = auth_of(&self.script, &self.key, &alternate.auth)
                    .inspect_err(|error| tracing::error!(%error, "an alternative sign-in was skipped"))
                    .ok()?;
                Some(AlternateAuth { key: alternate.key, label: alternate.label, button: alternate.button, site: alternate.site, auth })
            })
            .collect()
    }

    fn chooses(&self) -> Vec<String> {
        self.chooses.clone()
    }
}

#[async_trait]
impl EventSource for ScriptedEvents {
    async fn poll(&self, http: &dyn Http, creds: &Credentials, cursor: Option<&str>) -> Result<EventBatch, ActionError> {
        let online = Online { http, redirect: redirect() };
        let input = json!({ "creds": creds_of(creds), "cursor": cursor });
        let batch = invoke(&self.script, Arc::new(Local), &online, "poll", input).await?;
        let mut events = Vec::new();
        for mut event in batch["events"].as_array().cloned().unwrap_or_default() {
            event["connection"] = json!(creds.connection);
            events.push(serde_json::from_value::<ExternalEvent>(event).map_err(ActionError::decode)?);
        }
        Ok(EventBatch { events, cursor: batch["cursor"].as_str().unwrap_or_default().to_owned() })
    }
}

#[cfg(test)]
mod tests;
