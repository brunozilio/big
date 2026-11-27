use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::{Call, Config, Provider, ProviderError};

pub const SUBMIT: &str = "batchSubmit";
pub const POLL: &str = "batchPoll";
pub const STATE: &str = "batchState";
pub const RESULTS: &str = "batchResults";
pub const ANSWERS: &str = "batchAnswers";
pub const CANCEL: &str = "batchCancel";

pub const STEPS: [&str; 5] = [SUBMIT, POLL, STATE, RESULTS, ANSWERS];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Windows {
    pub gather: Duration,
    pub poll: Duration,
    pub wait: Duration,
}

impl Windows {
    pub fn configured() -> Self {
        Self::read(brand::section("batch"))
    }

    pub fn read(pairs: &[(&str, &str)]) -> Self {
        let number = |key: &str, fallback: u64| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .and_then(|(_, value)| value.trim().parse::<u64>().ok())
                .unwrap_or(fallback)
        };
        Self {
            gather: Duration::from_millis(number("window_ms", 2_000)),
            poll: Duration::from_millis(number("poll_ms", 10_000).max(1)),
            wait: Duration::from_millis(number("wait_ms", 90_000)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct State {
    pub id: String,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub results: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Answer {
    pub id: String,
    #[serde(default)]
    pub stream: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Shape {
    url: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    body: Option<String>,
}

fn call(provider: &dyn Provider, step: &str, input: Value) -> Result<Call, ProviderError> {
    let shape: Shape = read(provider, step, input)?;
    Ok(Call { url: shape.url, headers: shape.headers, body: shape.body })
}

fn read<T: serde::de::DeserializeOwned>(provider: &dyn Provider, step: &str, input: Value) -> Result<T, ProviderError> {
    let value = provider.batch(step, input)?;
    serde_json::from_value(value).map_err(|error| ProviderError::Invalid(format!("{} {step}: {error}", provider.id())))
}

pub fn submit(provider: &dyn Provider, config: &Config, requests: &[(String, Value)]) -> Result<Call, ProviderError> {
    let requests: Vec<Value> = requests.iter().map(|(id, body)| json!({ "id": id, "body": body })).collect();
    call(provider, SUBMIT, json!({ "config": config, "requests": requests }))
}

pub fn poll(provider: &dyn Provider, config: &Config, id: &str) -> Result<Call, ProviderError> {
    call(provider, POLL, json!({ "config": config, "id": id }))
}

pub fn cancel(provider: &dyn Provider, config: &Config, id: &str) -> Option<Call> {
    call(provider, CANCEL, json!({ "config": config, "id": id })).ok()
}

pub fn results(provider: &dyn Provider, config: &Config, url: &str) -> Result<Call, ProviderError> {
    call(provider, RESULTS, json!({ "config": config, "url": url }))
}

pub fn state(provider: &dyn Provider, value: &Value) -> Result<State, ProviderError> {
    read(provider, STATE, json!({ "value": value }))
}

pub fn answers(provider: &dyn Provider, text: &str) -> Result<Vec<Answer>, ProviderError> {
    read(provider, ANSWERS, json!({ "text": text }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_windows_come_from_the_configuration_with_defaults_for_what_is_missing() {
        let read = Windows::read(&[("window_ms", "5"), ("poll_ms", "0"), ("wait_ms", "nope")]);
        assert_eq!(read.gather, Duration::from_millis(5));
        assert_eq!(read.poll, Duration::from_millis(1));
        assert_eq!(read.wait, Duration::from_secs(90));
        assert!(Windows::configured().wait > Duration::ZERO);
    }

    #[test]
    fn a_provider_that_declares_nothing_does_not_batch() {
        let provider = crate::providers::get(domain::ModelProvider::new("grok-subscription"))
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(!provider.batches());
        assert!(state(provider, &json!({})).is_err());
    }
}
