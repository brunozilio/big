use std::sync::Arc;

use async_trait::async_trait;
use domain::Language;
use jiff::Timestamp;
use serde_json::Value;

use super::Grant;
use crate::descriptor::TokenField;
use crate::error::ActionError;
use crate::http::Http;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Authorization {
    pub url: String,
    pub state: String,
    pub verifier: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub carried: Value,
}

#[derive(Debug, Clone)]
pub enum Proceeded {
    Granted(Grant),
    Redirect(Authorization),
}

#[async_trait]
pub trait LoopbackSignIn: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;

    fn callback_port(&self) -> Option<u16> {
        None
    }

    fn fields(&self, _language: Language) -> Vec<TokenField> {
        Vec::new()
    }

    fn for_fields(&self, _fields: &Value, _language: Language) -> Result<Option<Loopback>, String> {
        Ok(None)
    }

    async fn without_sign_in(&self, _http: &dyn Http) -> Result<Option<Grant>, ActionError> {
        Ok(None)
    }

    async fn begin(&self, http: &dyn Http, port: u16) -> Result<Authorization, ActionError>;

    async fn exchange(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        code: &str,
        now: Timestamp,
    ) -> Result<Grant, ActionError>;

    async fn proceed(
        &self,
        _http: &dyn Http,
        _authorization: &Authorization,
        _query: &Value,
        _now: Timestamp,
    ) -> Result<Proceeded, ActionError> {
        Err(ActionError::auth("the sign-in came back without a code"))
    }

    async fn refresh(&self, http: &dyn Http, grant: &Grant, now: Timestamp) -> Result<Grant, ActionError>;
}

#[derive(Debug, Clone)]
pub struct Loopback(pub Arc<dyn LoopbackSignIn>);

impl Loopback {
    pub fn new(sign_in: impl LoopbackSignIn + 'static) -> Loopback {
        Loopback(Arc::new(sign_in))
    }

    pub fn entered(&self, fields: &Value, language: Language) -> Result<Loopback, String> {
        Ok(self.0.for_fields(fields, language)?.unwrap_or_else(|| self.clone()))
    }

    pub async fn finish(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        state: &str,
        code: &str,
        now: Timestamp,
    ) -> Result<Grant, ActionError> {
        if state != authorization.state {
            return Err(ActionError::auth("the answer does not belong to this sign-in"));
        }
        self.0.exchange(http, authorization, code, now).await
    }

    pub async fn proceed(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        query: &Value,
        now: Timestamp,
    ) -> Result<Proceeded, ActionError> {
        self.0.proceed(http, authorization, query, now).await
    }
}

impl PartialEq for Loopback {
    fn eq(&self, other: &Loopback) -> bool {
        self.0.name() == other.0.name()
    }
}

impl Eq for Loopback {}
