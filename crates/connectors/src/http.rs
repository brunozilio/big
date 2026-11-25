use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{ActionError, ErrorClass, classify};

pub const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Part {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

fn quoted(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).map(|c| if c == '"' { '\'' } else { c }).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    pub fn new(method: Method, url: impl Into<String>) -> HttpRequest {
        HttpRequest { method, url: url.into(), headers: Vec::new(), body: Vec::new() }
    }

    pub fn get(url: impl Into<String>) -> HttpRequest {
        HttpRequest::new(Method::Get, url)
    }

    pub fn post(url: impl Into<String>) -> HttpRequest {
        HttpRequest::new(Method::Post, url)
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> HttpRequest {
        self.headers.push((name.to_ascii_lowercase(), value.into()));
        self
    }

    pub fn bearer(self, token: &str) -> HttpRequest {
        self.header("authorization", format!("Bearer {token}"))
    }

    pub fn authorized(self, token: &str) -> HttpRequest {
        if token.starts_with("Basic ") { self.header("authorization", token.to_owned()) } else { self.bearer(token) }
    }

    pub fn body(mut self, content_type: &str, body: Vec<u8>) -> HttpRequest {
        self.body = body;
        self.header("content-type", content_type)
    }

    pub fn json(self, value: &impl Serialize) -> Result<HttpRequest, ActionError> {
        let body = serde_json::to_vec(value).map_err(ActionError::decode)?;
        Ok(self.body("application/json", body))
    }

    pub fn form(self, fields: &[(&str, &str)]) -> HttpRequest {
        let body = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).finish();
        self.body("application/x-www-form-urlencoded", body.into_bytes())
    }

    pub fn multipart(self, parts: &[Part]) -> HttpRequest {
        let mut seed = [0_u8; 12];
        let _ = getrandom::fill(&mut seed);
        let boundary = format!("----part{}", seed.iter().map(|byte| format!("{byte:02x}")).collect::<String>());
        let mut body = Vec::new();
        for part in parts {
            body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"", quoted(&part.name)).as_bytes());
            if let Some(filename) = &part.filename {
                body.extend_from_slice(format!("; filename=\"{}\"", quoted(filename)).as_bytes());
            }
            body.extend_from_slice(b"\r\n");
            if let Some(content_type) = &part.content_type {
                body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
            }
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(&part.bytes);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        self.body(&format!("multipart/form-data; boundary={boundary}"), body)
    }

    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    pub fn body_json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }

    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> HttpResponse {
        HttpResponse { status, headers: Vec::new(), body: body.into() }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> HttpResponse {
        self.headers.push((name.to_ascii_lowercase(), value.to_owned()));
        self
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    pub fn class(&self) -> Option<ErrorClass> {
        classify(self.status, &self.headers, jiff::Timestamp::now().as_second())
    }

    pub fn checked(self) -> Result<HttpResponse, ActionError> {
        match self.class() {
            None => Ok(self),
            Some(class) => {
                let body: String = self.text().chars().take(500).collect();
                Err(ActionError::new(class, format!("the service answered {}: {body}", self.status)))
            }
        }
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T, ActionError> {
        if self.body.is_empty() {
            return serde_json::from_value(Value::Null).map_err(ActionError::decode);
        }
        serde_json::from_slice(&self.body).map_err(ActionError::decode)
    }

    pub fn value(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

#[async_trait]
pub trait Http: Send + Sync + std::fmt::Debug {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ActionError>;
}

pub async fn fetch(http: &dyn Http, request: HttpRequest) -> Result<HttpResponse, ActionError> {
    http.send(request).await?.checked()
}

pub async fn fetch_json(http: &dyn Http, request: HttpRequest) -> Result<Value, ActionError> {
    fetch(http, request).await?.json()
}

#[derive(Debug, Clone)]
pub struct ReqwestHttp {
    client: reqwest::Client,
}

impl ReqwestHttp {
    pub fn new() -> Result<ReqwestHttp, ActionError> {
        let client = reqwest::Client::builder()
            .user_agent(brand::NAME)
            .timeout(TIMEOUT)
            .build()
            .map_err(|error| ActionError::permanent(error.to_string()))?;
        Ok(ReqwestHttp { client })
    }

    fn method(method: Method) -> reqwest::Method {
        match method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
        }
    }
}

fn transport(error: &reqwest::Error) -> ActionError {
    if error.is_builder() {
        ActionError::permanent(error.to_string())
    } else {
        ActionError::transient(format!("the request did not reach the service: {error}"))
    }
}

#[async_trait]
impl Http for ReqwestHttp {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ActionError> {
        let started = std::time::Instant::now();
        let method = format!("{:?}", request.method).to_uppercase();
        let address = platform::address::parts(&request.url);
        let sent = self.sent(request).await;
        let duration_ms = started.elapsed().as_millis() as u64;
        match &sent {
            Ok(response) => answered(&method, &address, response.status, duration_ms),
            Err(error) => tracing::event!(
                target: "audit", tracing::Level::WARN,
                component = "connector",
                http.request.method = %method,
                server.address = %address.host,
                url.path = %address.path,
                duration_ms,
                outcome = "failure",
                error.message = %error,
                "http.client.request"
            ),
        }
        sent
    }
}

fn answered(method: &str, address: &platform::address::Address, status: u16, duration_ms: u64) {
    let (host, path) = (address.host.as_str(), address.path.as_str());
    if status >= 400 {
        tracing::event!(target: "audit", tracing::Level::WARN, component = "connector", http.request.method = method, server.address = host, url.path = path, http.response.status_code = status, duration_ms, outcome = "failure", "http.client.request");
    } else {
        tracing::event!(target: "audit", tracing::Level::INFO, component = "connector", http.request.method = method, server.address = host, url.path = path, http.response.status_code = status, duration_ms, outcome = "success", "http.client.request");
    }
}

impl ReqwestHttp {
    async fn sent(&self, request: HttpRequest) -> Result<HttpResponse, ActionError> {
        let mut builder = self.client.request(ReqwestHttp::method(request.method), &request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let response = builder.body(request.body).send().await.map_err(|error| transport(&error))?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| (name.as_str().to_owned(), String::from_utf8_lossy(value.as_bytes()).into_owned()))
            .collect();
        let body = response.bytes().await.map_err(|error| transport(&error))?.to_vec();
        Ok(HttpResponse { status, headers, body })
    }
}

#[derive(Debug, Clone)]
struct Route {
    method: Method,
    url_part: String,
    answer: Result<HttpResponse, ActionError>,
    repeat: bool,
}

#[derive(Debug, Default)]
pub struct FakeHttp {
    routes: Mutex<Vec<Route>>,
    sent: Mutex<Vec<HttpRequest>>,
}

impl FakeHttp {
    pub fn new() -> FakeHttp {
        FakeHttp::default()
    }

    pub fn once(self, method: Method, url_part: &str, status: u16, body: &str) -> FakeHttp {
        self.answer(method, url_part, Ok(HttpResponse::new(status, body)), false)
    }

    pub fn always(self, method: Method, url_part: &str, status: u16, body: &str) -> FakeHttp {
        self.answer(method, url_part, Ok(HttpResponse::new(status, body)), true)
    }

    pub fn respond(self, method: Method, url_part: &str, response: HttpResponse) -> FakeHttp {
        self.answer(method, url_part, Ok(response), false)
    }

    pub fn fail(self, method: Method, url_part: &str, error: ActionError) -> FakeHttp {
        self.answer(method, url_part, Err(error), false)
    }

    fn answer(
        self,
        method: Method,
        url_part: &str,
        answer: Result<HttpResponse, ActionError>,
        repeat: bool,
    ) -> FakeHttp {
        let route = Route { method, url_part: url_part.to_owned(), answer, repeat };
        self.routes.lock().unwrap_or_else(PoisonError::into_inner).push(route);
        self
    }

    pub fn sent(&self) -> Vec<HttpRequest> {
        self.sent.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn sent_to(&self, method: Method, url_part: &str) -> Vec<HttpRequest> {
        self.sent().into_iter().filter(|request| request.method == method && request.url.contains(url_part)).collect()
    }
}

#[async_trait]
impl Http for FakeHttp {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ActionError> {
        self.sent.lock().unwrap_or_else(PoisonError::into_inner).push(request.clone());
        let mut routes = self.routes.lock().unwrap_or_else(PoisonError::into_inner);
        let found = routes
            .iter()
            .position(|route| route.method == request.method && request.url.contains(&route.url_part))
            .ok_or_else(|| {
                ActionError::permanent(format!("no fake answer for {} {}", request.method.as_str(), request.url))
            })?;
        let answer = routes[found].answer.clone();
        if !routes[found].repeat {
            routes.remove(found);
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multipart_body_carries_each_field_and_file_between_boundaries() {
        let file = Part { name: "file".into(), filename: Some("a\"b.png".into()), content_type: Some("image/png".into()), bytes: vec![0, 159, 1] };
        let field = Part { name: "note".into(), bytes: b"oi".to_vec(), ..Part::default() };
        let request = HttpRequest::post("https://h").multipart(&[file, field]);
        let kind = request.header_value("content-type").unwrap_or_default().to_owned();
        let boundary = kind.strip_prefix("multipart/form-data; boundary=").unwrap_or_default().to_owned();
        assert!(boundary.len() > 10, "{kind}");
        let mut expected = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a'b.png\"\r\nContent-Type: image/png\r\n\r\n").into_bytes();
        expected.extend_from_slice(&[0, 159, 1]);
        expected.extend_from_slice(format!("\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\noi\r\n--{boundary}--\r\n").as_bytes());
        assert_eq!(request.body, expected);
    }

    #[tokio::test]
    async fn the_fake_answers_in_order_and_records_what_was_sent() {
        let http = FakeHttp::new().once(Method::Get, "/a", 200, "1").once(Method::Get, "/a", 200, "2");
        let first = http.send(HttpRequest::get("https://x/a")).await.map(|r| r.text());
        let second = http.send(HttpRequest::get("https://x/a")).await.map(|r| r.text());
        assert_eq!((first, second), (Ok("1".to_owned()), Ok("2".to_owned())));
        assert!(http.send(HttpRequest::get("https://x/a")).await.is_err());
        assert_eq!(http.sent_to(Method::Get, "/a").len(), 3);
    }

    #[tokio::test]
    async fn fetch_turns_an_error_status_into_its_class() {
        let http = FakeHttp::new().once(Method::Get, "/x", 503, "down");
        let error = fetch(&http, HttpRequest::get("https://h/x")).await.err();
        assert_eq!(error.map(|e| e.class), Some(ErrorClass::Transient));
    }

    #[test]
    fn a_form_body_is_url_encoded() {
        let request = HttpRequest::post("https://h").form(&[("a", "b c"), ("d", "&")]);
        assert_eq!(request.body_text(), "a=b+c&d=%26");
        assert_eq!(request.header_value("Content-Type"), Some("application/x-www-form-urlencoded"));
    }
}
