use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use domain::connection::ConnectionId;
use tokio::time::Instant;

use crate::error::{ActionError, ErrorClass, retry_after};
use crate::http::{Http, HttpRequest, HttpResponse};

#[derive(Debug, Default)]
pub struct RateLimiter {
    until: Mutex<HashMap<ConnectionId, Instant>>,
}

impl RateLimiter {
    pub fn new() -> RateLimiter {
        RateLimiter::default()
    }

    pub fn hold(&self, connection: &ConnectionId, wait: Duration) {
        let until = Instant::now() + wait;
        let mut all = self.until.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = all.entry(connection.clone()).or_insert(until);
        if *entry < until {
            *entry = until;
        }
    }

    pub fn waiting(&self, connection: &ConnectionId) -> Option<Duration> {
        let all = self.until.lock().unwrap_or_else(PoisonError::into_inner);
        let until = *all.get(connection)?;
        let now = Instant::now();
        (until > now).then(|| until - now)
    }

    pub async fn acquire(&self, connection: &ConnectionId) {
        while let Some(wait) = self.waiting(connection) {
            tokio::time::sleep(wait).await;
        }
    }

    pub fn observe(&self, connection: &ConnectionId, response: &HttpResponse) {
        let now = jiff::Timestamp::now().as_second();
        if let Some(wait) = retry_after(&response.headers, now) {
            self.hold(connection, wait);
        }
    }

    pub fn learn(&self, connection: &ConnectionId, error: &ActionError) {
        if let ErrorClass::RateLimited { retry_after } = error.class {
            self.hold(connection, retry_after);
        }
    }
}

#[derive(Debug)]
pub struct Metered<'a> {
    pub inner: &'a dyn Http,
    pub limiter: &'a RateLimiter,
    pub connection: &'a ConnectionId,
}

#[async_trait]
impl Http for Metered<'_> {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ActionError> {
        self.limiter.acquire(self.connection).await;
        let response = self.inner.send(request).await?;
        self.limiter.observe(self.connection, &response);
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{FakeHttp, Method};

    #[tokio::test(start_paused = true)]
    async fn a_held_connection_waits_and_others_do_not() {
        let limiter = RateLimiter::new();
        let one = ConnectionId::from("one");
        let two = ConnectionId::from("two");
        limiter.hold(&one, Duration::from_secs(30));
        limiter.hold(&one, Duration::from_secs(5));
        assert!(limiter.waiting(&one).is_some_and(|wait| wait > Duration::from_secs(29)));
        assert_eq!(limiter.waiting(&two), None);
        let started = Instant::now();
        limiter.acquire(&one).await;
        assert!(started.elapsed() >= Duration::from_secs(30));
        assert_eq!(limiter.waiting(&one), None);
    }

    #[tokio::test(start_paused = true)]
    async fn an_exhausted_quota_or_retry_after_holds_the_connection() {
        let limiter = RateLimiter::new();
        let id = ConnectionId::from("c");
        let quota = HttpResponse::new(200, "{}").with_header("x-ratelimit-remaining", "0");
        let http = FakeHttp::new().respond(Method::Get, "/x", quota);
        let metered = Metered { inner: &http, limiter: &limiter, connection: &id };
        assert!(metered.send(HttpRequest::get("https://h/x")).await.is_ok());
        assert!(limiter.waiting(&id).is_some());
        let other = ConnectionId::from("d");
        limiter.learn(&other, &ActionError::rate_limited(Duration::from_secs(9), "slow"));
        assert!(limiter.waiting(&other).is_some_and(|wait| wait <= Duration::from_secs(9)));
    }
}
