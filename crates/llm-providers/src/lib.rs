extern crate self as llm_providers;

pub mod added_tools;
pub mod batch;
pub mod catalog;
mod config;
mod data_handling;
pub mod limits;
pub mod model;
pub mod oauth;
mod owners;
pub mod prices;
pub mod providers;
pub mod request;
pub mod scripted;
pub mod sign_in;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
pub mod transcript;
pub mod web_search;

pub use owners::{EFFORTS, efforts, later_discount, logo, maker, owner, owns, priced, serves};

use domain::{Access, ModelOption};
use serde_json::Value;

pub use catalog::{Call, Page};
pub use config::Config;
pub use oauth::{TokenRequest, Tokens};
pub use sign_in::Host;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    #[error("not signed in: {0}")]
    Unauthorized(String),
    #[error("{0}")]
    Invalid(String),
    #[error("the sign-in callback port {0} is already in use by another app")]
    PortInUse(u16),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Refusal {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub message: String,
}

pub trait Provider: Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn label(&self) -> &'static str {
        self.name()
    }
    fn logo(&self) -> &'static str {
        ""
    }
    fn access(&self) -> Access;
    fn rank(&self) -> u8;
    fn slot(&self) -> u128;
    fn window(&self) -> u64;
    fn wire(&self, model: Option<&ModelOption>) -> &'static str;
    fn base(&self, config: &Config) -> String;
    fn headers(&self, config: &Config) -> Vec<(String, String)>;
    fn path(&self, model: Option<&ModelOption>) -> &'static str;
    fn shape(
        &self,
        model: Option<&ModelOption>,
        body: &mut Value,
        effort: &str,
        intent: &Value,
    ) -> Result<(), ProviderError>;
    fn catalog(&self, config: &Config, after: &str) -> Call;
    fn page(&self, value: &Value) -> Result<Page, ProviderError>;

    fn bare(&self, raw: &str) -> Config {
        Config { api_key: raw.trim().to_owned(), ..Config::default() }
    }

    fn check(&self, _config: &Config) -> Result<(), ProviderError> {
        Ok(())
    }

    fn fields(&self, _language: domain::Language) -> Value {
        Value::Null
    }

    fn bind(&self, _config: &Config, _body: &mut Value) {}

    fn owns(&self) -> &[&'static str] {
        &[]
    }

    fn makers(&self) -> &[(&'static str, &'static str)] {
        &[]
    }

    fn alias(&self, _name: &str) -> Option<&'static str> {
        None
    }

    fn serves(&self, model: &str) -> bool {
        serves(self, model)
    }

    fn small_model(&self) -> Option<&'static str> {
        None
    }

    fn handling(&self, _model: &str) -> domain::data_policy::Handling {
        domain::data_policy::Handling::default()
    }

    fn later_discount(&self) -> Option<f64> {
        None
    }

    fn prices(&self) -> &prices::Prices {
        &prices::NONE
    }

    fn price_date(&self) -> &'static str {
        ""
    }

    fn basis(&self) -> domain::billing::Basis {
        if self.access().subscription() { domain::billing::Basis::Subscription } else { domain::billing::Basis::Api }
    }

    fn search_price(&self) -> Option<u64> {
        None
    }

    fn batches(&self) -> bool {
        false
    }

    fn batch(&self, step: &str, _input: Value) -> Result<Value, ProviderError> {
        Err(ProviderError::Invalid(format!("{} has no {step} for batches", self.name())))
    }

    fn web_search(&self, _model: &str) -> Option<Value> {
        None
    }

    fn search(&self, _config: &Config, _query: &str) -> Option<Call> {
        None
    }

    fn limits(&self, _config: &Config) -> Option<Call> {
        None
    }

    fn windows(
        &self,
        _value: &Value,
        _language: domain::Language,
    ) -> Result<domain::billing::PlanUsage, ProviderError> {
        Err(ProviderError::Invalid(format!("{} does not report plan limits", self.name())))
    }

    fn hits(&self, _value: &Value) -> Result<Vec<web_search::Hit>, ProviderError> {
        Err(ProviderError::Invalid(format!("{} has no web search of its own", self.name())))
    }

    fn tools_in_conversation(&self, _model: &str) -> bool {
        false
    }

    fn turn_headers(&self, _model: Option<&ModelOption>, _body: &Value) -> Vec<(String, String)> {
        Vec::new()
    }

    fn retries_after_refresh(&self, _status: u16) -> bool {
        false
    }

    fn refusal(&self, _status: u16, _body: &str) -> Option<Refusal> {
        None
    }

    fn framing(&self) -> &'static str {
        "ndjson"
    }

    fn frame(&self, _state: &Value, _frame: &Value) -> Result<Value, ProviderError> {
        Err(ProviderError::Invalid(format!("{} does not read stream frames", self.name())))
    }

    fn finish(&self, models: Vec<ModelOption>) -> Vec<ModelOption> {
        models
    }

    fn refresh(&self, _previous: &Tokens) -> Option<TokenRequest> {
        None
    }

    fn tokens(
        &self,
        status: u16,
        _body: &str,
        _previous: Option<&Tokens>,
        _now: jiff::Timestamp,
    ) -> Result<Tokens, ProviderError> {
        Err(ProviderError::Unauthorized(format!("{} returned HTTP {status} and keeps no tokens", self.name())))
    }

    fn sign_in(&self, _host: &dyn Host) -> Result<Config, ProviderError> {
        Err(ProviderError::Invalid(format!("{} connects with an API key", self.name())))
    }
}

pub fn id(provider: &dyn Provider) -> domain::ModelProvider {
    domain::ModelProvider::new(provider.id())
}

pub fn release_local_models() {
    #[cfg(feature = "local-models")]
    local_models::unload();
}
