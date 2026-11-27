use std::sync::Arc;

use async_trait::async_trait;
use domain::Language;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{DeviceCode, Grant, Polled};
use crate::descriptor::TokenField;
use crate::error::ActionError;
use crate::http::Http;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rename {
    pub find: String,
    pub replace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Started {
    pub code: DeviceCode,
    #[serde(default)]
    pub carried: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capture: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rename: Vec<Rename>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub captured: Value,
}

#[async_trait]
pub trait DeviceSignIn: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;

    fn fields(&self, _language: Language) -> Vec<TokenField> {
        Vec::new()
    }

    async fn start(&self, http: &dyn Http, fields: &Value, language: Language) -> Result<Started, ActionError>;

    async fn poll(&self, http: &dyn Http, started: &Started, now: Timestamp) -> Result<Polled, ActionError>;

    async fn refresh(&self, http: &dyn Http, grant: &Grant, now: Timestamp) -> Result<Grant, ActionError>;
}

#[derive(Debug, Clone)]
pub struct Stepped(pub Arc<dyn DeviceSignIn>);

impl Stepped {
    pub fn new(sign_in: impl DeviceSignIn + 'static) -> Stepped {
        Stepped(Arc::new(sign_in))
    }
}

impl PartialEq for Stepped {
    fn eq(&self, other: &Stepped) -> bool {
        self.0.name() == other.0.name()
    }
}

impl Eq for Stepped {}
