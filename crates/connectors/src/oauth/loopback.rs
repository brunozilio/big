use std::sync::Arc;

use async_trait::async_trait;
use jiff::Timestamp;

use super::{Authorization, Grant};
use crate::error::ActionError;
use crate::http::Http;

#[async_trait]
pub trait LoopbackSignIn: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;

    async fn begin(&self, http: &dyn Http, port: u16) -> Result<Authorization, ActionError>;

    async fn exchange(
        &self,
        http: &dyn Http,
        authorization: &Authorization,
        code: &str,
        now: Timestamp,
    ) -> Result<Grant, ActionError>;

    async fn refresh(&self, http: &dyn Http, grant: &Grant, now: Timestamp) -> Result<Grant, ActionError>;
}

#[derive(Debug, Clone)]
pub struct Loopback(pub Arc<dyn LoopbackSignIn>);

impl Loopback {
    pub fn new(sign_in: impl LoopbackSignIn + 'static) -> Loopback {
        Loopback(Arc::new(sign_in))
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
}

impl PartialEq for Loopback {
    fn eq(&self, other: &Loopback) -> bool {
        self.0.name() == other.0.name()
    }
}

impl Eq for Loopback {}
