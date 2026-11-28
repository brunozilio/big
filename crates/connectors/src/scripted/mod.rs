mod bridge;
mod host;
mod secrets;
mod sessions;
mod sign_in;
mod socket;

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use domain::Language;
use domain::connection::{Capability, ProviderKey};
use serde::Deserialize;
use serde_json::{Map, Value, json};

pub use bridge::{Answers, Script};
use bridge::{WRAPPER, invoke, wrapper};
pub use host::Local;
use host::{Kept, Online};
use secrets::ScriptedSecrets;
use sessions::McpSessions;
pub use sign_in::ScriptedSignIn;
use sign_in::{SIGN_IN, redirect};

use crate::config::{self, Configuration};
use crate::descriptor::{ActionSpec, AlternateAuth, AuthSpec, Presentation, ProviderDescriptor, TokenSpec};
use crate::error::ActionError;
use crate::http::Http;
use crate::oauth::{DeviceApp, Loopback, Stepped};
use crate::provider::{AccountOption, Credentials, EventBatch, EventSource, ExternalEvent, Provider, SecretSource};

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
    #[serde(default)]
    requires: domain::data_policy::Requires,
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
    requires: domain::data_policy::Requires,
    events: Option<ScriptedEvents>,
    secrets: Option<ScriptedSecrets>,
    answers: Arc<dyn Answers>,
    sessions: Arc<McpSessions>,
    shown: Mutex<Vec<(String, Language, Value)>>,
}

#[derive(Debug)]
struct ScriptedEvents {
    script: Script,
    answers: Arc<dyn Answers>,
    sessions: Arc<McpSessions>,
}

fn data_of(config: Map<String, Value>, configuration: &Configuration) -> Value {
    let brand = json!({ "NAME": brand::NAME, "COMMAND": brand::COMMAND });
    json!({ "config": config, "connectors": configuration.section("connectors"), "brand": brand })
}

fn script_of(
    modules: Vec<(String, String)>,
    named: &str,
    configuration: &Configuration,
    overrides: &Map<String, Value>,
) -> Result<Script, String> {
    let mut given = configuration.section(named);
    given.extend(overrides.clone());
    let mut script = Script { modules: Arc::new(modules), data: data_of(given.clone(), configuration) };
    let schema = script.invoke_here(config::EXPORT, &json!({}), Arc::new(Local)).map_err(|error| error.message)?;
    if !schema.is_null() {
        script.data["config"] = Value::Object(config::applied(named, &schema, &given)?);
    }
    Ok(script)
}

fn auth_of(script: &Script, key: &str, auth: &Value) -> Result<AuthSpec, String> {
    let kind = auth["kind"].as_str().unwrap_or_default();
    let parsed = |what: &str| format!("{key}: the {what} sign-in is not valid");
    match kind {
        "token" => serde_json::from_value::<TokenSpec>(auth.clone()).map(AuthSpec::Token).map_err(|_| parsed(kind)),
        "device" => serde_json::from_value::<DeviceApp>(auth.clone()).map(AuthSpec::Device).map_err(|_| parsed(kind)),
        "loopback" | "steps" => {
            let state = auth.get("state").cloned().unwrap_or(json!({}));
            let using = auth["using"].as_str().filter(|using| !using.is_empty()).unwrap_or(SIGN_IN);
            let sign_in = ScriptedSignIn::new(script.clone(), key, using, state);
            if kind == "steps" {
                Ok(AuthSpec::Steps(Stepped::new(sign_in)))
            } else {
                Ok(AuthSpec::Loopback(Loopback::new(sign_in)))
            }
        }
        other => Err(format!("{key}: no sign-in is called `{other}`")),
    }
}

fn logo_of(root: Option<&std::path::Path>, folder: &str, logo: &str) -> String {
    if logo.trim_start().starts_with('<') || logo.is_empty() {
        return logo.to_owned();
    }
    let path = format!("{folder}{logo}");
    content::bytes_in(root, &path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default()
}

impl ScriptedProvider {
    pub fn load(entry: &str) -> Result<ScriptedProvider, String> {
        ScriptedProvider::load_with(content::folder().as_deref(), entry, &Configuration::read())
    }

    pub fn load_in(root: Option<&std::path::Path>, entry: &str) -> Result<ScriptedProvider, String> {
        ScriptedProvider::load_with(root, entry, &Configuration::built())
    }

    pub fn load_with(
        root: Option<&std::path::Path>,
        entry: &str,
        configuration: &Configuration,
    ) -> Result<ScriptedProvider, String> {
        ScriptedProvider::configured(root, entry, configuration, &Map::new())
    }

    pub(crate) fn configured(
        root: Option<&std::path::Path>,
        entry: &str,
        configuration: &Configuration,
        overrides: &Map<String, Value>,
    ) -> Result<ScriptedProvider, String> {
        let mut modules = content::modules_in(root, entry)?;
        modules.push((WRAPPER.to_owned(), wrapper(entry)));
        let folder = entry.rsplit_once('/').map_or(String::new(), |(folder, _)| format!("{folder}/"));
        let named = folder.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_owned();
        let script = script_of(modules, &named, configuration, overrides)?;
        let local: Arc<dyn script::Host> = Arc::new(Local);
        let declared = script.invoke_here("provider", &json!({}), local).map_err(|error| error.message)?;
        let mut declared = crate::manifest::migrated(root, declared).map_err(|error| error.to_string())?;
        crate::manifest::read(&mut declared, entry).map_err(|error| error.to_string())?;
        let declared: Declared = serde_json::from_value(declared).map_err(|error| format!("{entry}: {error}"))?;
        let key = if declared.key.is_empty() { named } else { declared.key.clone() };
        let polls = script.kind("poll") == "function";
        let descriptor = ProviderDescriptor {
            key: ProviderKey::from(key.as_str()),
            name: declared.name,
            logo: logo_of(root, &folder, &declared.logo),
            capabilities: declared.capabilities.iter().map(|name| Capability::from(name.as_str())).collect(),
            auth: auth_of(&script, &key, &declared.auth)?,
            actions: declared.actions,
        };
        let answers: Arc<dyn Answers> = Arc::new(Local);
        let sessions = Arc::new(McpSessions::default());
        let events = polls.then(|| ScriptedEvents {
            script: script.clone(),
            answers: Arc::clone(&answers),
            sessions: Arc::clone(&sessions),
        });
        let secrets = (script.kind(secrets::READ) == "function")
            .then(|| ScriptedSecrets { script: script.clone(), sessions: Arc::clone(&sessions) });
        Ok(ScriptedProvider {
            key,
            script,
            descriptor,
            chooses: declared.chooses,
            requires: declared.requires,
            events,
            secrets,
            answers,
            sessions,
            shown: Mutex::default(),
        })
    }

    pub fn with_answers(mut self, answers: Arc<dyn Answers>) -> ScriptedProvider {
        if let Some(events) = &mut self.events {
            events.answers = Arc::clone(&answers);
        }
        self.answers = answers;
        self
    }

    fn local(&self, name: &str, language: Language) -> Value {
        let shown = |memo: &[(String, Language, Value)]| {
            memo.iter()
                .find(|(known, spoken, _)| known == name && *spoken == language)
                .map(|(_, _, value)| value.clone())
        };
        if let Some(found) = shown(&self.shown.lock().unwrap_or_else(PoisonError::into_inner)) {
            return found;
        }
        let input = json!({ "language": language.code() });
        let value = self.script.invoke_here(name, &input, Arc::new(Local)).unwrap_or_default();
        let mut memo = self.shown.lock().unwrap_or_else(PoisonError::into_inner);
        if shown(&memo).is_none() {
            memo.push((name.to_owned(), language, value.clone()));
        }
        value
    }

    async fn online(&self, http: &dyn Http, name: &str, input: Value) -> Result<Value, ActionError> {
        let online = Online::pooled(http, redirect(), Arc::clone(&self.sessions), &input);
        let answers = Kept::of(&self.answers, &input);
        invoke(&self.script, answers, &online, name, input).await
    }
}

pub fn evaluate(root: Option<&std::path::Path>, entry: &str, function: &str, input: &Value) -> Result<Value, String> {
    let mut modules = content::modules_in(root, entry)?;
    modules.push((WRAPPER.to_owned(), wrapper(entry)));
    let script = Script {
        modules: Arc::new(modules),
        data: json!({ "brand": { "COMMAND": brand::COMMAND }, "connectors": Configuration::built().section("connectors") }),
    };
    if script.kind(function) != "function" {
        return Ok(Value::Null);
    }
    script.invoke_here(function, input, Arc::new(Local)).map_err(|error| error.message)
}

fn creds_of(creds: &Credentials) -> Value {
    json!({ "connection": creds.connection, "token": creds.access_token, "settings": creds.settings })
}

fn entries() -> Vec<String> {
    let entries = content::listed(FOLDER);
    entries
        .into_iter()
        .filter(|path| {
            path.strip_prefix(FOLDER)
                .and_then(|rest| rest.strip_suffix(ENTRY))
                .is_some_and(|name| name.matches('/').count() == 1 && name.ends_with('/'))
        })
        .collect()
}

pub fn discover() -> Vec<Arc<dyn Provider>> {
    let entries = entries();
    let root = content::folder();
    let configuration = Configuration::read();
    let loaded = platform::parallel::ordered(&entries, |entry| {
        ScriptedProvider::load_with(root.as_deref(), entry, &configuration)
    });
    let mut found: Vec<Arc<dyn Provider>> = Vec::new();
    for (entry, loaded) in entries.iter().zip(loaded) {
        match loaded {
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
        let input =
            json!({ "action": action, "args": args, "creds": creds_of(creds), "idempotencyKey": idempotency_key });
        self.online(http, "call", input).await
    }

    async fn accounts(&self, http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        let found = self.online(http, "accounts", json!({ "creds": creds_of(creds) })).await?;
        serde_json::from_value(found).map_err(ActionError::decode)
    }

    fn events(&self) -> Option<&dyn EventSource> {
        self.events.as_ref().map(|events| events as &dyn EventSource)
    }

    fn secrets(&self) -> Option<&dyn SecretSource> {
        self.secrets.as_ref().map(|secrets| secrets as &dyn SecretSource)
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
                Some(AlternateAuth {
                    key: alternate.key,
                    label: alternate.label,
                    button: alternate.button,
                    site: alternate.site,
                    auth,
                })
            })
            .collect()
    }

    fn chooses(&self) -> Vec<String> {
        self.chooses.clone()
    }

    fn requires(&self) -> domain::data_policy::Requires {
        self.requires.clone()
    }

    fn messages(&self, language: Language) -> std::collections::BTreeMap<String, String> {
        serde_json::from_value(self.local("messages", language)).unwrap_or_default()
    }

    fn secret_patterns(&self) -> domain::redact::Rules {
        let declared = self.local("secretPatterns", Language::default());
        if declared.is_null() {
            return domain::redact::Rules::default();
        }
        domain::redact::Rules::read(&declared)
            .inspect_err(
                |error| tracing::warn!(connector = self.key, %error, "the connector's secret patterns do not read"),
            )
            .unwrap_or_default()
    }
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
        let input = json!({ "creds": creds_of(creds), "cursor": cursor, "watch": watch });
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

#[cfg(test)]
mod tests;
