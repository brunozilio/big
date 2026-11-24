use std::time::Duration;

use tokio_util::sync::CancellationToken;

pub use super::body::{Body, Chunk};
use super::cancellable_http::{self, Ask};
use super::{BoxFuture, ModelError};

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Response").field("status", &self.status).finish_non_exhaustive()
    }
}

impl Response {
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.clone())
    }

    pub async fn text(self, cancel: &CancellationToken) -> Result<String, ModelError> {
        self.body.text(cancel, SHORT_ANSWER_AT_MOST).await
    }
}

pub const SHORT_ANSWER_AT_MOST: usize = 32 * 1024 * 1024;

impl Response {}

pub const STREAM_IDLE: Duration = Duration::from_secs(300);

pub fn idle_from(value: Option<&str>) -> Duration {
    value
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map_or(STREAM_IDLE, Duration::from_secs)
}

pub fn stream_idle() -> Duration {
    idle_from(Some(brand::agent::STREAM_IDLE_SECONDS))
}

#[derive(Debug, Clone, Copy)]
pub struct Outgoing<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub headers: &'a [(String, String)],
    pub body: Option<&'a str>,
    pub streaming: bool,
}

pub trait Http: Send + Sync {
    fn send<'a>(
        &'a self,
        out: Outgoing<'a>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        let body = out.body.unwrap_or_default();
        match (out.method.to_ascii_uppercase().as_str(), out.body.is_some(), out.streaming) {
            ("GET", false, true) => self.get_stream(out.url, out.headers, cancel),
            ("GET", false, false) => self.request_cancellable(out.url, out.headers, None, cancel),
            ("POST", _, true) => self.post_streaming(out.url, out.headers, body, cancel),
            ("POST", _, false) => self.post_cancellable(out.url, out.headers, body, cancel),
            (method, ..) => Box::pin(std::future::ready(Err(ModelError::Network(format!(
                "{method} with this body is not supported by this HTTP transport"
            ))))),
        }
    }

    fn post_streaming<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        body: &'a str,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        Box::pin(async move {
            if cancel.is_cancelled() {
                return Err(ModelError::Cancelled);
            }
            self.post_model(url, headers, body)
        })
    }

    fn post_cancellable<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        body: &'a str,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        Box::pin(async move {
            if cancel.is_cancelled() {
                return Err(ModelError::Cancelled);
            }
            self.post_model(url, headers, body)
        })
    }

    fn get(&self, _url: &str, _headers: &[(String, String)]) -> Result<Response, String> {
        Err("GET is not supported by this HTTP transport".into())
    }

    fn get_stream<'a>(
        &'a self,
        _url: &'a str,
        _headers: &'a [(String, String)],
        _cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        Box::pin(std::future::ready(Err(ModelError::Network(
            "streaming GET is not supported by this HTTP transport".into(),
        ))))
    }

    fn one_hop<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        body: Option<&'a str>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        self.request_cancellable(url, headers, body, cancel)
    }

    fn request_cancellable<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        body: Option<&'a str>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        Box::pin(async move {
            if cancel.is_cancelled() {
                return Err(ModelError::Cancelled);
            }
            match body {
                Some(body) => self.post(url, headers, body),
                None => self.get(url, headers),
            }
            .map_err(ModelError::Network)
        })
    }

    fn addresses(&self, host: &str) -> Result<Vec<std::net::IpAddr>, String> {
        super::web_client::system_addresses(host)
    }

    fn post_model(&self, url: &str, headers: &[(String, String)], body: &str) -> Result<Response, ModelError> {
        self.post(url, headers, body).map_err(ModelError::Network)
    }

    fn post(&self, url: &str, headers: &[(String, String)], body: &str) -> Result<Response, String>;
}

#[derive(Debug, Default)]
pub struct NetHttp;

pub const SHORT_TIMEOUT: Duration = Duration::from_secs(15);
pub const LISTENED_AT_MOST: Duration = Duration::from_secs(12 * 3600);

pub fn reason(error: ModelError) -> String {
    match error {
        ModelError::Network(reason) => reason,
        error => error.to_string(),
    }
}

impl NetHttp {
    fn short(url: &str, headers: &[(String, String)], body: Option<&str>) -> Result<Response, String> {
        let never = CancellationToken::new();
        let ask = Ask { total: Some(SHORT_TIMEOUT), ..Ask::new(url, headers, body) };
        platform::runtime::block_on(async {
            let response = cancellable_http::fetch(ask, &never).await?;
            let (status, headers) = (response.status, response.headers.clone());
            let text = response.text(&never).await?;
            Ok(Response { status, headers, body: Body::from(text) })
        })
        .map_err(|error| error.to_string())?
        .map_err(reason)
    }
}

impl Http for NetHttp {
    fn send<'a>(
        &'a self,
        out: Outgoing<'a>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        let ask = Ask { method: out.method, ..Ask::new(out.url, out.headers, out.body) };
        let ask = if out.streaming {
            Ask { idle: Some(stream_idle()), ..ask }
        } else {
            Ask { total: Some(cancellable_http::STREAM_TIMEOUT), ..ask }
        };
        Box::pin(cancellable_http::fetch(ask, cancel))
    }

    fn post_streaming<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        body: &'a str,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        Box::pin(cancellable_http::fetch(
            Ask { idle: Some(stream_idle()), ..Ask::new(url, headers, Some(body)) },
            cancel,
        ))
    }

    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<Response, String> {
        Self::short(url, headers, None)
    }

    fn one_hop<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        body: Option<&'a str>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        let ask = Ask { total: Some(SHORT_TIMEOUT), one_hop: true, ..Ask::new(url, headers, body) };
        Box::pin(cancellable_http::fetch(ask, cancel))
    }

    fn request_cancellable<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        body: Option<&'a str>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        let ask = Ask { total: Some(SHORT_TIMEOUT), ..Ask::new(url, headers, body) };
        Box::pin(cancellable_http::fetch(ask, cancel))
    }

    fn post_cancellable<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        body: &'a str,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        let ask = Ask { total: Some(cancellable_http::STREAM_TIMEOUT), ..Ask::new(url, headers, Some(body)) };
        Box::pin(cancellable_http::fetch(ask, cancel))
    }

    fn get_stream<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        Box::pin(cancellable_http::fetch(Ask { total: Some(LISTENED_AT_MOST), ..Ask::new(url, headers, None) }, cancel))
    }

    fn post(&self, url: &str, headers: &[(String, String)], body: &str) -> Result<Response, String> {
        Self::short(url, headers, Some(body))
    }
}

#[cfg(any(test, feature = "test-support"))]
pub use super::scripted_http::{ScriptedHttp, Sent};

#[cfg(test)]
#[path = "http_line_tests.rs"]
mod line_tests;
