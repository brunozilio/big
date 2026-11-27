use super::super::{
    BoxFuture, CancellationToken, KeySource, ModelError,
    http::{Http, Response},
};
use super::{Config, oauth};
use llm_providers::{Provider, catalog, request};
use serde_json::Value;
use std::io::Read;
use std::sync::Arc;

use tokio::sync::Mutex;

pub(super) struct Transport {
    pub provider: &'static dyn Provider,
    pub http: Arc<dyn Http>,
    pub keys: Arc<dyn KeySource>,
    pub turns: super::turns::Turns,
    pub cached: Mutex<Option<(String, Config)>>,
    pub model: Option<domain::ModelOption>,
    pub windows: std::sync::Mutex<llm_providers::batch::Windows>,
}

impl Transport {
    pub(super) fn config(&self, force: bool) -> Result<Config, ModelError> {
        platform::runtime::block_on(self.config_under(force, &CancellationToken::new()))
            .unwrap_or_else(|error| Err(ModelError::Network(error.to_string())))
    }

    pub(super) async fn config_under(&self, force: bool, cancel: &CancellationToken) -> Result<Config, ModelError> {
        let key = self.keys.key()?;
        let raw = key.secret();
        let mut cached = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(ModelError::Cancelled),
            held = self.cached.lock() => held,
        };
        let config = match cached.as_ref() {
            Some((old, config)) if old == raw => config.clone(),
            _ => Config::parse(self.provider, raw)?,
        };
        let config = if config.needs_refresh(self.provider, force) && config.auth.is_some() {
            self.refreshed(raw, config, cancel).await?
        } else {
            config
        };
        *cached = Some((raw.to_owned(), config.clone()));
        Ok(config)
    }

    async fn refreshed(
        &self,
        seen: &str,
        mut config: Config,
        cancel: &CancellationToken,
    ) -> Result<Config, ModelError> {
        let keys = Arc::clone(&self.keys);
        let held = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(ModelError::Cancelled),
            held = tokio::task::spawn_blocking(move || keys.hold_for_refresh()) => {
                held.map_err(|error| ModelError::Network(error.to_string()))??
            }
        };
        let latest = self.keys.key()?;
        if latest.secret() != seen {
            let theirs = Config::parse(self.provider, latest.secret())?;
            if !theirs.needs_refresh(self.provider, false) {
                return Ok(theirs);
            }
            config = theirs;
        }
        let previous = config.auth.as_ref().ok_or_else(|| ModelError::Unauthorized("no tokens to refresh".into()))?;
        let tokens = match oauth::refresh(self.http.as_ref(), self.provider, previous, cancel).await? {
            oauth::Renewal::Fresh(tokens) => tokens,
            oauth::Renewal::Refused(error) => {
                self.keys.lapsed(true);
                return Err(error);
            }
        };
        config.auth = Some(tokens);
        self.keys.replace(&serde_json::to_string(&config).map_err(|e| ModelError::Provider(e.to_string()))?)?;
        drop(held);
        self.keys.lapsed(false);
        Ok(config)
    }

    pub(super) fn models(&self) -> Result<Vec<domain::ModelOption>, ModelError> {
        catalog::collect(self.provider, |after| {
            let _turn = self.turns.take();
            let call = self.provider.catalog(&self.config(false)?, after);
            match &call.body {
                Some(body) => self.http.post(&call.url, &call.headers, body),
                None => self.http.get(&call.url, &call.headers),
            }
            .map_err(ModelError::Network)
            .and_then(json_response)
        })
    }

    pub(super) fn limits(&self, language: domain::Language) -> Result<Option<domain::billing::PlanUsage>, ModelError> {
        if self.provider.limits(&Config::default()).is_none() {
            return Ok(None);
        }
        let _turn = self.turns.take();
        let mut response = self.limits_once(false)?;
        if self.provider.retries_after_refresh(response.status) {
            drop(response);
            response = self.limits_once(true)?;
        }
        status(response.status)?;
        let value: Value = serde_json::from_str(&read(response, llm_providers::limits::LIMIT)?)
            .map_err(|_| ModelError::Provider("the plan limits answered with something other than JSON".into()))?;
        Ok(Some(self.provider.windows(&value, language)?))
    }

    fn limits_once(&self, force: bool) -> Result<Response, ModelError> {
        let config = self.config(force)?;
        let call = self.provider.limits(&config).ok_or_else(|| ModelError::Provider("no plan limits".into()))?;
        match &call.body {
            Some(body) => self.http.post(&call.url, &call.headers, body),
            None => self.http.get(&call.url, &call.headers),
        }
        .map_err(ModelError::Network)
    }

    pub(super) async fn search(
        &self,
        query: &str,
        cancel: &CancellationToken,
    ) -> Option<Result<Vec<llm_providers::web_search::Hit>, ModelError>> {
        self.provider.search(&Config::default(), query)?;
        let Some(_turn) = self.turns.take_unless(cancel).await else { return Some(Err(ModelError::Cancelled)) };
        Some(self.searched(query, cancel).await)
    }

    async fn searched(
        &self,
        query: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<llm_providers::web_search::Hit>, ModelError> {
        let mut response = self.search_once(query, false, cancel).await?;
        if self.provider.retries_after_refresh(response.status) {
            drop(response);
            response = self.search_once(query, true, cancel).await?;
        }
        status(response.status)?;
        let text = response.body.text(cancel, SEARCH_LIMIT).await?;
        let value: Value = serde_json::from_str(&text)
            .map_err(|_| ModelError::Provider("the web search answered with something other than JSON".into()))?;
        self.provider.hits(&value).map_err(|error| ModelError::Provider(error.to_string()))
    }

    async fn search_once(&self, query: &str, force: bool, cancel: &CancellationToken) -> Result<Response, ModelError> {
        let config = self.config_under(force, cancel).await?;
        let call = self.provider.search(&config, query).ok_or(ModelError::Cancelled)?;
        self.http.post_cancellable(&call.url, &call.headers, call.body.as_deref().unwrap_or_default(), cancel).await
    }
}

const SEARCH_LIMIT: usize = 2 << 20;

impl Http for Transport {
    fn post_streaming<'a>(
        &'a self,
        _url: &'a str,
        _headers: &'a [(String, String)],
        body: &'a str,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Response, ModelError>> {
        Box::pin(self.send(body, cancel))
    }
    fn post(&self, url: &str, headers: &[(String, String)], body: &str) -> Result<Response, String> {
        self.post_model(url, headers, body).map_err(|error| error.to_string())
    }
    fn post_model(&self, _url: &str, _headers: &[(String, String)], body: &str) -> Result<Response, ModelError> {
        platform::runtime::block_on(self.send(body, &CancellationToken::new()))
            .unwrap_or_else(|error| Err(ModelError::Network(error.to_string())))
    }
}

impl Transport {
    async fn send(&self, body: &str, cancel: &CancellationToken) -> Result<Response, ModelError> {
        let mut body: Value = serde_json::from_str(body).map_err(|e| ModelError::Provider(e.to_string()))?;
        if body["intent"]["priority"] == "later" && self.provider.batches() {
            match self.batched(body.clone(), cancel).await {
                Err(ModelError::Cancelled) => return Err(ModelError::Cancelled),
                Err(error) => tracing::info!(provider = self.provider.id(), %error, "a low-priority call goes now"),
                answered => return answered,
            }
        }
        let Some(_turn) = self.turns.take_unless(cancel).await else { return Err(ModelError::Cancelled) };
        request::shape(self.provider, self.model.as_ref(), &mut body)?;
        let turn_headers = self.provider.turn_headers(self.model.as_ref(), &body);
        let shaped = body;
        let bound = |config: &Config| {
            let mut body = shaped.clone();
            self.provider.bind(config, &mut body);
            body.to_string()
        };
        let path = self.provider.path(self.model.as_ref());
        let headers = |config: &Config| {
            let mut headers = config.headers(self.provider);
            headers.extend(turn_headers.iter().cloned());
            headers
        };
        let mut config = self.config_under(false, cancel).await?;
        let mut url = format!("{}{path}", config.base(self.provider));
        let mut response = self.http.post_streaming(&url, &headers(&config), &bound(&config), cancel).await?;
        if self.provider.retries_after_refresh(response.status) {
            drop(response);
            config = self.config_under(true, cancel).await?;
            url = format!("{}{path}", config.base(self.provider));
            response = self.http.post_streaming(&url, &headers(&config), &bound(&config), cancel).await?;
        }
        let limit = if response.status < 300 { request::STREAM_LIMIT } else { request::ERROR_LIMIT };
        response.body = response.body.bounded(limit);
        Ok(response)
    }
}

impl Transport {
    async fn batched(&self, mut body: Value, cancel: &CancellationToken) -> Result<Response, ModelError> {
        request::shape(self.provider, self.model.as_ref(), &mut body)?;
        let config = self.config_under(false, cancel).await?;
        self.provider.bind(&config, &mut body);
        let mut hasher = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&config.headers(self.provider), &mut hasher);
        let key = format!("{}:{}", self.provider.id(), std::hash::Hasher::finish(&hasher));
        let windows = *self.windows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let job = super::batch::Job { provider: self.provider, http: Arc::clone(&self.http), config, windows };
        let stream = super::batch::answer(job, key, body, cancel).await?;
        Ok(Response { status: 200, headers: Vec::new(), body: stream.into() })
    }
}

pub(super) fn read(mut response: Response, limit: u64) -> Result<String, ModelError> {
    let mut text = String::new();
    response.body.by_ref().take(limit + 1).read_to_string(&mut text).map_err(|e| ModelError::Network(e.to_string()))?;
    if text.len() as u64 > limit {
        return Err(ModelError::Provider("provider response exceeds size limit".into()));
    }
    Ok(text)
}

pub(super) fn status(status: u16) -> Result<(), ModelError> {
    match status {
        200..=299 => Ok(()),
        401 | 403 => Err(ModelError::Unauthorized(format!("provider returned HTTP {status}"))),
        429 | 500..=599 => Err(ModelError::Overloaded(format!("provider returned HTTP {status}"))),
        _ => Err(ModelError::Provider(format!("provider returned HTTP {status}"))),
    }
}

fn json_response(response: Response) -> Result<Value, ModelError> {
    status(response.status)?;
    serde_json::from_str(&read(response, catalog::PAGE_LIMIT)?)
        .map_err(|_| ModelError::Provider("invalid model catalog JSON".into()))
}
