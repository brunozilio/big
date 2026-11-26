use jiff::Timestamp;
use std::time::Duration;

use super::{ProviderError, TokenRequest};

pub trait Host {
    fn post(&self, url: &str, headers: &[(String, String)], body: &str) -> Result<(u16, String), ProviderError>;
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<(u16, String), ProviderError>;
    fn open(&self, url: String, code: Option<String>);
    fn listen(&self, port: u16, path: &str) -> Result<Box<dyn Listener>, ProviderError>;
    fn random(&self, bytes: usize) -> Result<String, ProviderError>;
    fn sleep(&self, duration: Duration);
    fn now(&self) -> Timestamp;
    fn fields(&self) -> serde_json::Value {
        serde_json::Value::Null
    }
    fn download(&self, request: &serde_json::Value) -> Result<serde_json::Value, ProviderError> {
        let request: local_models::Request =
            serde_json::from_value(request.clone()).map_err(|error| ProviderError::Invalid(error.to_string()))?;
        let transfer = local_models::download(request).map_err(|error| ProviderError::Invalid(error.to_string()))?;
        serde_json::to_value(transfer).map_err(|error| ProviderError::Invalid(error.to_string()))
    }
}

pub trait Listener {
    fn redirect_uri(&self) -> String;
    fn port(&self) -> u16;
    fn next(&self, state: &str, others: &[&str]) -> Result<Box<dyn Answer>, ProviderError>;
    fn next_by(&self, key: &str, state: &str, others: &[&str]) -> Result<Box<dyn Answer>, ProviderError> {
        if key == "state" {
            return self.next(state, others);
        }
        Err(ProviderError::Invalid(format!("this sign-in cannot tell its callback by `{key}`")))
    }
}

pub trait Answer {
    fn path(&self) -> &str;
    fn query(&self) -> &str;
    fn code(&self) -> Result<String, ProviderError>;
    fn redirect(self: Box<Self>, to: &str);
    fn finish(self: Box<Self>, result: Result<(), String>);
    fn signed_in(self: Box<Self>, _account: Option<String>) {
        self.finish(Ok(()));
    }
}

pub fn post(host: &dyn Host, request: &TokenRequest) -> Result<(u16, String), ProviderError> {
    host.post(&request.url, &[("content-type".to_owned(), request.content_type.to_owned())], &request.body)
}
