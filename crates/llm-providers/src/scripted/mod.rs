mod bridge;
mod host;
mod sign_in;
#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex, PoisonError};

use domain::billing::PlanUsage;
use domain::{Access, Language, ModelOption};
use jiff::Timestamp;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

pub use bridge::Script;
use bridge::{WRAPPER, wrapper};
use host::Local;

use crate::web_search::Hit;
use crate::{Call, Config, Host, Page, Provider, ProviderError, TokenRequest, Tokens};

pub const FOLDER: &str = "providers/";
pub const ENTRY: &str = "index.ts";

pub fn intern(text: &str) -> &'static str {
    static KNOWN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut known = KNOWN.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(found) = known.iter().find(|known| **known == text) {
        return found;
    }
    let leaked: &'static str = Box::leak(text.to_owned().into_boxed_str());
    known.push(leaked);
    leaked
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Declared {
    id: String,
    name: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    logo: String,
    #[serde(default)]
    vendor: String,
    #[serde(default)]
    access: Access,
    #[serde(default)]
    rank: u8,
    #[serde(default)]
    slot: String,
    #[serde(default)]
    window: u64,
    #[serde(default)]
    wire: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    small_model: Option<String>,
    #[serde(default)]
    serves_all: bool,
}

#[derive(Debug)]
pub struct ScriptedProvider {
    script: Script,
    functions: Vec<String>,
    id: &'static str,
    name: &'static str,
    label: &'static str,
    logo: &'static str,
    vendor: &'static str,
    access: Access,
    rank: u8,
    slot: u128,
    window: u64,
    wire: &'static str,
    path: &'static str,
    small: Option<&'static str>,
    serves_all: bool,
}

fn section(name: &str) -> Value {
    let pairs: Map<String, Value> =
        brand::section(name).iter().map(|(key, value)| ((*key).to_owned(), json!(value))).collect();
    Value::Object(pairs)
}

fn data_of(name: &str) -> Value {
    json!({
        "config": section(name),
        "brand": { "NAME": brand::NAME, "COMMAND": brand::COMMAND },
        "platform": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
    })
}

fn logo_of(root: Option<&std::path::Path>, folder: &str, logo: &str) -> String {
    if logo.trim_start().starts_with('<') || logo.is_empty() {
        return logo.to_owned();
    }
    content::bytes_in(root, &format!("{folder}{logo}"))
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

fn slot_of(text: &str) -> u128 {
    let digits: String = text.trim_start_matches("0x").chars().filter(|c| *c != '_').collect();
    u128::from_str_radix(&digits, 16).unwrap_or_default()
}

fn call_of(value: &Value) -> Option<Call> {
    let url = value["url"].as_str().filter(|url| !url.is_empty())?.to_owned();
    let headers = serde_json::from_value(value["headers"].clone()).unwrap_or_default();
    let body = value["body"].as_str().map(str::to_owned);
    Some(Call { url, headers, body })
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
        let script = Script { modules: Arc::new(modules), data: data_of(&named) };
        let local: Arc<dyn script::Host> = Arc::new(Local { provider: "" });
        let declared = script.invoke("provider", json!({}), Arc::clone(&local)).map_err(|error| error.to_string())?;
        let declared: Declared = serde_json::from_value(declared).map_err(|error| format!("{entry}: {error}"))?;
        let functions = script.exported(local);
        let label = if declared.label.is_empty() { declared.name.clone() } else { declared.label.clone() };
        Ok(ScriptedProvider {
            script,
            functions,
            id: intern(&declared.id),
            name: intern(&declared.name),
            label: intern(&label),
            logo: intern(&logo_of(root, &folder, &declared.logo)),
            vendor: intern(&declared.vendor),
            access: declared.access,
            rank: declared.rank,
            slot: slot_of(&declared.slot),
            window: declared.window,
            wire: intern(&declared.wire),
            path: intern(&declared.path),
            small: declared.small_model.as_deref().map(intern),
            serves_all: declared.serves_all,
        })
    }

    fn offers(&self, function: &str) -> bool {
        self.functions.iter().any(|known| known == function)
    }

    fn local(&self) -> Arc<dyn script::Host> {
        Arc::new(Local { provider: self.id })
    }

    fn invoke(&self, function: &str, input: Value) -> Result<Value, ProviderError> {
        self.script.invoke(function, input, self.local())
    }

    fn read<T: DeserializeOwned>(&self, function: &str, input: Value) -> Result<T, ProviderError> {
        let value = self.invoke(function, input)?;
        serde_json::from_value(value).map_err(|error| ProviderError::Invalid(format!("{} {function}: {error}", self.id)))
    }

    fn quiet<T: DeserializeOwned + Default>(&self, function: &str, input: Value) -> T {
        if !self.offers(function) {
            return T::default();
        }
        self.read(function, input).unwrap_or_else(|error| {
            tracing::error!(provider = self.id, function, %error, "a provider script failed");
            T::default()
        })
    }

    fn named(&self, function: &str, model: Option<&ModelOption>, fallback: &'static str) -> &'static str {
        if !self.offers(function) {
            return fallback;
        }
        let chosen: String = self.quiet(function, json!({ "model": model }));
        if chosen.is_empty() { fallback } else { intern(&chosen) }
    }

    fn optional_call(&self, function: &str, input: Value) -> Option<Call> {
        let value: Value = self.quiet(function, input);
        call_of(&value)
    }
}

pub fn discover() -> Vec<ScriptedProvider> {
    discover_in(content::folder().as_deref())
}

pub fn discover_in(root: Option<&std::path::Path>) -> Vec<ScriptedProvider> {
    let entries = content::listed_in(root, FOLDER);
    let entries = entries.iter().filter(|path| {
        path.strip_prefix(FOLDER)
            .and_then(|rest| rest.strip_suffix(ENTRY))
            .is_some_and(|name| name.matches('/').count() == 1 && name.ends_with('/'))
    });
    let mut found: Vec<ScriptedProvider> = Vec::new();
    for entry in entries {
        match ScriptedProvider::load_in(root, entry) {
            Ok(provider) if found.iter().all(|known| known.id != provider.id) => found.push(provider),
            Ok(provider) => tracing::error!(entry, id = provider.id, "a provider repeats another's id"),
            Err(error) => tracing::error!(%error, entry, "a provider could not be loaded"),
        }
    }
    found
}

impl Provider for ScriptedProvider {
    fn id(&self) -> &'static str {
        self.id
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn label(&self) -> &'static str {
        self.label
    }
    fn logo(&self) -> &'static str {
        self.logo
    }
    fn vendor(&self) -> &'static str {
        self.vendor
    }
    fn access(&self) -> Access {
        self.access
    }
    fn rank(&self) -> u8 {
        self.rank
    }
    fn slot(&self) -> u128 {
        self.slot
    }
    fn window(&self) -> u64 {
        self.window
    }
    fn wire(&self, model: Option<&ModelOption>) -> &'static str {
        self.named("wire", model, self.wire)
    }
    fn base(&self, config: &Config) -> String {
        self.quiet("base", json!({ "config": config }))
    }
    fn headers(&self, config: &Config) -> Vec<(String, String)> {
        self.quiet("headers", json!({ "config": config }))
    }
    fn path(&self, model: Option<&ModelOption>) -> &'static str {
        self.named("path", model, self.path)
    }
    fn shape(&self, model: Option<&ModelOption>, body: &mut Value, effort: &str) -> Result<(), ProviderError> {
        if self.offers("shape") {
            *body = self.invoke("shape", json!({ "model": model, "body": body, "effort": effort }))?;
        }
        Ok(())
    }
    fn catalog(&self, config: &Config, after: &str) -> Call {
        self.optional_call("catalog", json!({ "config": config, "after": after }))
            .unwrap_or_else(|| Call { url: String::new(), headers: Vec::new(), body: None })
    }
    fn page(&self, value: &Value) -> Result<Page, ProviderError> {
        let page: Value = self.read("page", json!({ "value": value }))?;
        let models = serde_json::from_value(page["models"].clone())
            .map_err(|error| ProviderError::Invalid(format!("{} page: {error}", self.id)))?;
        let next = page["next"].as_str().map(str::to_owned);
        Ok(Page { models, next })
    }
    fn bare(&self, raw: &str) -> Config {
        if !self.offers("bare") {
            return Config { api_key: raw.trim().to_owned(), ..Config::default() };
        }
        self.quiet("bare", json!({ "raw": raw }))
    }
    fn check(&self, config: &Config) -> Result<(), ProviderError> {
        if self.offers("check") {
            self.invoke("check", json!({ "config": config }))?;
        }
        Ok(())
    }
    fn fields(&self, language: Language) -> Value {
        self.quiet("fields", json!({ "language": language.code() }))
    }
    fn bind(&self, config: &Config, body: &mut Value) {
        if self.offers("bind") {
            *body = self.quiet("bind", json!({ "config": config, "body": body }));
        }
    }
    fn serves(&self, model: &str) -> bool {
        if self.serves_all {
            return true;
        }
        let model = model.rsplit(':').next().unwrap_or(model);
        crate::vendor(self.vendor).map_or_else(|| crate::owner(model).is_some(), |vendor| vendor.owns(model))
    }
    fn small_model(&self) -> Option<&'static str> {
        self.small.or_else(|| crate::vendor(self.vendor).and_then(crate::Vendor::small))
    }
    fn web_search(&self, model: &str) -> Option<Value> {
        let native: Value = self.quiet("webSearch", json!({ "name": model }));
        (!native.is_null()).then_some(native)
    }
    fn search(&self, config: &Config, query: &str) -> Option<Call> {
        self.optional_call("search", json!({ "config": config, "query": query }))
    }
    fn limits(&self, config: &Config) -> Option<Call> {
        self.optional_call("limits", json!({ "config": config }))
    }
    fn windows(&self, value: &Value, language: Language) -> Result<PlanUsage, ProviderError> {
        if !self.offers("windows") {
            return Err(ProviderError::Invalid(format!("{} does not report plan limits", self.name)));
        }
        self.read("windows", json!({ "value": value, "language": language.code() }))
    }
    fn hits(&self, value: &Value) -> Result<Vec<Hit>, ProviderError> {
        if !self.offers("hits") {
            return Err(ProviderError::Invalid(format!("{} has no web search of its own", self.name)));
        }
        self.read("hits", json!({ "value": value }))
    }
    fn tools_in_conversation(&self, model: &str) -> bool {
        self.quiet("toolsInConversation", json!({ "name": model }))
    }
    fn turn_headers(&self, model: Option<&ModelOption>, body: &Value) -> Vec<(String, String)> {
        self.quiet("turnHeaders", json!({ "model": model, "body": body }))
    }
    fn retries_after_refresh(&self, status: u16) -> bool {
        self.quiet("retriesAfterRefresh", json!({ "status": status }))
    }
    fn finish(&self, models: Vec<ModelOption>) -> Vec<ModelOption> {
        if !self.offers("finish") {
            return models;
        }
        let input = json!({ "models": models });
        self.read("finish", input).unwrap_or_else(|error| {
            tracing::error!(provider = self.id, %error, "a provider script failed to finish its catalog");
            models
        })
    }
    fn refresh(&self, previous: &Tokens) -> Option<TokenRequest> {
        let value: Value = self.quiet("refresh", json!({ "previous": previous }));
        let url = value["url"].as_str().filter(|url| !url.is_empty())?.to_owned();
        let content_type = intern(value["contentType"].as_str().unwrap_or("application/json"));
        Some(TokenRequest { url, content_type, body: value["body"].as_str().unwrap_or_default().to_owned() })
    }
    fn tokens(&self, status: u16, body: &str, previous: Option<&Tokens>, now: Timestamp) -> Result<Tokens, ProviderError> {
        if !self.offers("tokens") {
            return Err(ProviderError::Unauthorized(format!("{} returned HTTP {status} and keeps no tokens", self.name)));
        }
        let input = json!({ "status": status, "body": body, "previous": previous, "now": now.to_string() });
        self.read("tokens", input)
    }
    fn sign_in(&self, host: &dyn Host) -> Result<Config, ProviderError> {
        if !self.offers("signIn") {
            return Err(ProviderError::Invalid(format!("{} connects with an API key", self.name)));
        }
        let config: Config = serde_json::from_value(sign_in::run(&self.script, Local { provider: self.id }, host)?)
            .map_err(|error| ProviderError::Invalid(format!("{} sign-in: {error}", self.id)))?;
        config.validate(self)?;
        Ok(config)
    }
}
