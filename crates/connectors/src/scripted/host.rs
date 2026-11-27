use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use jiff::Timestamp;
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::bridge::{Answers, Waits, encoded};
use crate::error::ActionError;
use crate::http::{Http, HttpRequest, HttpResponse, Method, Part};
use super::sessions::{Ask, Identity, McpSessions};
use crate::oauth::mcp::McpSignIn;
use crate::oauth::{Authorization, CodeSignIn, Grant, LoopbackSignIn};

fn bytes_of(text: &str, encoding: &str) -> Result<Vec<u8>, String> {
    match encoding {
        "" | "text" => Ok(text.as_bytes().to_vec()),
        "base64" => STANDARD.decode(text).map_err(|error| error.to_string()),
        "base64url" => URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')).map_err(|error| error.to_string()),
        "hex" => (0..text.len())
            .step_by(2)
            .map(|at| text.get(at..at + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok()))
            .collect::<Option<Vec<u8>>>()
            .ok_or_else(|| "not hex".to_owned()),
        other => Err(format!("no encoding is called {other}")),
    }
}

fn text_of(bytes: &[u8], encoding: &str) -> Result<String, String> {
    match encoding {
        "" | "hex" => Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect()),
        "text" => String::from_utf8(bytes.to_vec()).map_err(|error| error.to_string()),
        "base64" => Ok(STANDARD.encode(bytes)),
        "base64url" => Ok(URL_SAFE_NO_PAD.encode(bytes)),
        other => Err(format!("no encoding is called {other}")),
    }
}

fn field<'a>(arguments: &'a Value, name: &str) -> &'a str {
    arguments[name].as_str().unwrap_or_default()
}

fn input(arguments: &Value, name: &str) -> Result<Vec<u8>, String> {
    bytes_of(field(arguments, name), field(arguments, &format!("{name}Encoding")))
}

fn digest(arguments: &Value) -> Result<Value, String> {
    let data = input(arguments, "data")?;
    text_of(&Sha256::digest(&data), field(arguments, "output")).map(Value::String)
}

fn hmac(arguments: &Value) -> Result<Value, String> {
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &input(arguments, "key")?);
    let tag = ring::hmac::sign(&key, &input(arguments, "data")?);
    text_of(tag.as_ref(), field(arguments, "output")).map(Value::String)
}

fn random(arguments: &Value) -> Result<Value, String> {
    let size = arguments["bytes"].as_u64().unwrap_or(32).min(1024);
    let mut bytes = vec![0_u8; usize::try_from(size).unwrap_or(32)];
    getrandom::fill(&mut bytes).map_err(|error| error.to_string())?;
    text_of(&bytes, field(arguments, "output")).map(Value::String)
}

fn convert(arguments: &Value) -> Result<Value, String> {
    let bytes = bytes_of(field(arguments, "data"), field(arguments, "from"))?;
    text_of(&bytes, field(arguments, "to")).map(Value::String)
}

fn kept(connection: Option<&str>, function: &str, arguments: &Value) -> Result<Value, String> {
    let Some(connection) = connection else {
        return Err("the connector memory is kept per connection, so it is offered only in calls, polls and accounts"
            .to_owned());
    };
    let Some(memory) = crate::memory::installed() else {
        return Err("the connector memory is not kept by this app".to_owned());
    };
    let name = field(arguments, "name");
    if function == "memory.get" {
        return memory.get(connection, name);
    }
    memory.set(connection, name, &arguments["value"]).map(|()| Value::Null)
}

#[derive(Debug)]
pub struct Kept {
    connection: Option<String>,
    inner: Arc<dyn Answers>,
}

impl Kept {
    pub fn of(inner: &Arc<dyn Answers>, input: &Value) -> Arc<dyn Answers> {
        let connection = input["creds"]["connection"].as_str().map(str::to_owned);
        Arc::new(Kept { connection, inner: Arc::clone(inner) })
    }
}

fn evaluated(connection: Option<&str>, arguments: &Value) -> Result<Value, String> {
    let Some(connection) = connection else {
        return Err(
            "the browser is reached per connection, so it is offered only in calls, polls and accounts".to_owned()
        );
    };
    let Some(browser) = crate::browser::installed() else {
        return Err("no browser is connected to this app for connectors".to_owned());
    };
    browser.eval(connection, arguments)
}

impl Answers for Kept {
    fn now(&self, function: &str, arguments: &Value) -> Option<Result<Value, String>> {
        if function.starts_with("memory.") {
            return Some(kept(self.connection.as_deref(), function, arguments));
        }
        if function == "browser.eval" {
            return Some(evaluated(self.connection.as_deref(), arguments));
        }
        self.inner.now(function, arguments)
    }
}

fn p256_pair(private: &str) -> Result<EcdsaKeyPair, String> {
    let pkcs8 = STANDARD.decode(private).map_err(|error| error.to_string())?;
    EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &pkcs8, &SystemRandom::new())
        .map_err(|error| format!("the private key is not a P-256 key: {error}"))
}

fn p256_public(pair: &EcdsaKeyPair) -> Result<Value, String> {
    let point = pair.public_key().as_ref();
    let (Some(x), Some(y)) = (point.get(1..33), point.get(33..65)) else {
        return Err("the public key is not an uncompressed P-256 point".to_owned());
    };
    Ok(json!({ "kty": "EC", "crv": "P-256", "x": URL_SAFE_NO_PAD.encode(x), "y": URL_SAFE_NO_PAD.encode(y) }))
}

fn ecdsa_generate() -> Result<Value, String> {
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
        .map_err(|error| error.to_string())?;
    let private = STANDARD.encode(pkcs8.as_ref());
    let public = p256_public(&p256_pair(&private)?)?;
    Ok(json!({ "private": private, "public": public }))
}

fn ecdsa_public(arguments: &Value) -> Result<Value, String> {
    p256_public(&p256_pair(field(arguments, "private"))?)
}

fn ecdsa_sign(arguments: &Value) -> Result<Value, String> {
    let pair = p256_pair(field(arguments, "private"))?;
    let signed = pair.sign(&SystemRandom::new(), &input(arguments, "data")?).map_err(|error| error.to_string())?;
    text_of(signed.as_ref(), field(arguments, "output")).map(Value::String)
}

#[derive(Debug, Clone, Default)]
pub struct Local;

impl Answers for Local {
    fn now(&self, function: &str, arguments: &Value) -> Option<Result<Value, String>> {
        Some(match function {
            "log" => {
                tracing::info!(target: "connector", "{}", arguments.as_str().unwrap_or_default());
                Ok(Value::Null)
            }
            "crypto.sha256" => digest(arguments),
            "crypto.hmac" => hmac(arguments),
            "crypto.random" => random(arguments),
            "crypto.ecdsa.generate" => ecdsa_generate(),
            "crypto.ecdsa.public" => ecdsa_public(arguments),
            "crypto.ecdsa.sign" => ecdsa_sign(arguments),
            "encoding.convert" => convert(arguments),
            "clock.now" => Ok(json!(Timestamp::now().as_second())),
            "net.site" => Ok(json!(domain::site::site(field(arguments, "url")))),
            "net.safeEndpoint" => Ok(json!(domain::mcp_oauth::endpoint_ok(field(arguments, "url")))),
            "browser.eval" => {
                Err("the browser is reached per connection, so it is offered only in calls, polls and accounts"
                    .to_owned())
            }
            "memory.get" | "memory.set" => {
                Err("the connector memory is kept per connection, so it is offered only in calls, polls and accounts"
                    .to_owned())
            }
            "clock.millis" => Ok(json!(Timestamp::now().as_millisecond())),
            _ => return None,
        })
    }
}

impl script::Host for Local {
    fn call(&self, function: &str, arguments: Value) -> Result<Value, String> {
        self.now(function, &arguments)
            .unwrap_or_else(|| Err(format!("`{function}` needs the network, offered only in actions")))
    }
}

fn method(name: &str) -> Result<Method, ActionError> {
    Ok(match name.to_ascii_uppercase().as_str() {
        "" | "GET" => Method::Get,
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "PATCH" => Method::Patch,
        "DELETE" => Method::Delete,
        other => return Err(ActionError::permanent(format!("the method {other} is not offered"))),
    })
}

fn request(arguments: &Value) -> Result<HttpRequest, ActionError> {
    let mut request = HttpRequest::new(method(field(arguments, "method"))?, field(arguments, "url"));
    for (name, value) in arguments["headers"].as_object().into_iter().flatten() {
        request = request.header(name, value.as_str().map_or_else(|| value.to_string(), str::to_owned));
    }
    if let Some(text) = arguments["body"].as_str() {
        request.body = text.as_bytes().to_vec();
    } else if let Some(encoded) = arguments["base64"].as_str() {
        request.body = STANDARD.decode(encoded).map_err(|error| ActionError::permanent(error.to_string()))?;
    } else if let Some(listed) = arguments["parts"].as_array() {
        let parts = listed.iter().map(part).collect::<Result<Vec<Part>, ActionError>>()?;
        request = request.multipart(&parts);
    }
    Ok(request)
}

fn part(found: &Value) -> Result<Part, ActionError> {
    let text = |name: &str| found[name].as_str().filter(|text| !text.is_empty()).map(str::to_owned);
    let bytes = match found["base64"].as_str() {
        Some(encoded) => STANDARD.decode(encoded).map_err(|error| ActionError::permanent(error.to_string()))?,
        None => field(found, "value").as_bytes().to_vec(),
    };
    Ok(Part {
        name: field(found, "name").to_owned(),
        filename: text("filename"),
        content_type: text("contentType"),
        bytes,
    })
}

fn joined_headers(answer: &HttpResponse) -> Map<String, Value> {
    let mut joined: Map<String, Value> = Map::new();
    for (name, value) in &answer.headers {
        let name = name.to_ascii_lowercase();
        let glue = if name == "set-cookie" { "\n" } else { ", " };
        let next = match joined.get(&name).and_then(Value::as_str) {
            Some(before) => format!("{before}{glue}{value}"),
            None => value.clone(),
        };
        joined.insert(name, json!(next));
    }
    joined
}

fn response(answer: &HttpResponse, binary: bool) -> Value {
    let headers = joined_headers(answer);
    let mut shown = json!({ "status": answer.status, "headers": headers, "body": answer.text() });
    if binary {
        shown["base64"] = json!(STANDARD.encode(&answer.body));
    }
    shown
}

fn parsed<T: serde::de::DeserializeOwned>(value: &Value, what: &str) -> Result<T, ActionError> {
    serde_json::from_value(value.clone()).map_err(|error| ActionError::permanent(format!("{what}: {error}")))
}

fn now_of(arguments: &Value) -> Timestamp {
    arguments["now"].as_i64().and_then(|second| Timestamp::from_second(second).ok()).unwrap_or_else(Timestamp::now)
}

#[derive(Debug)]
pub struct Online<'a> {
    http: &'a dyn Http,
    redirect: String,
    sessions: Arc<McpSessions>,
    connection: String,
    sockets: super::socket::Sockets,
}

impl<'a> Online<'a> {
    pub fn new(http: &'a dyn Http, redirect: String) -> Online<'a> {
        Online::pooled(http, redirect, Arc::default(), &Value::Null)
    }

    pub fn pooled(http: &'a dyn Http, redirect: String, sessions: Arc<McpSessions>, input: &Value) -> Online<'a> {
        let connection = input["creds"]["connection"].as_str().unwrap_or_default().to_owned();
        Online { http, redirect, sessions, connection, sockets: super::socket::Sockets::default() }
    }

    async fn mcp(&self, arguments: &Value, ask: Ask) -> Result<Value, ActionError> {
        let identity = Identity {
            connection: self.connection.clone(),
            url: field(arguments, "url").to_owned(),
            token: field(arguments, "token").to_owned(),
        };
        self.sessions.ask(self.http, &identity, &ask).await
    }

    fn sign_in(&self, arguments: &Value) -> McpSignIn {
        let scopes = arguments["scopes"].as_array().into_iter().flatten().filter_map(Value::as_str);
        let redirect = arguments["redirect"].as_str().filter(|redirect| !redirect.is_empty());
        McpSignIn {
            server: field(arguments, "server").to_owned(),
            redirect: redirect.map_or_else(|| self.redirect.clone(), str::to_owned),
            client_name: brand::connectors::OAUTH_CLIENT_NAME.to_owned(),
            scopes: scopes.map(str::to_owned).collect(),
        }
    }

    fn code_sign_in(&self, arguments: &Value) -> CodeSignIn {
        let scopes = arguments["scopes"].as_array().into_iter().flatten().filter_map(Value::as_str);
        let redirect = arguments["redirect"].as_str().filter(|redirect| !redirect.is_empty());
        CodeSignIn {
            authorize_url: field(arguments, "authorize_url").to_owned(),
            token_url: field(arguments, "token_url").to_owned(),
            client_id: field(arguments, "client_id").to_owned(),
            redirect: redirect.map_or_else(|| self.redirect.clone(), str::to_owned),
            scopes: scopes.map(str::to_owned).collect(),
        }
    }

    async fn reach(&self, function: &str, arguments: &Value) -> Result<Value, ActionError> {
        match function {
            "http" => match self.http.send(request(arguments)?).await {
                Ok(answer) => Ok(response(&answer, arguments["binary"].as_bool().unwrap_or(false))),
                Err(error) => Ok(json!({ "failed": error })),
            },
            "mcp.tools" => self.mcp(arguments, Ask::Tools).await,
            "mcp.call" => {
                let ask = Ask::Call(field(arguments, "tool").to_owned(), arguments["arguments"].clone());
                self.mcp(arguments, ask).await
            }
            "mcp.connect" => self.mcp(arguments, Ask::Connect).await,
            "socket.open" => self.sockets.open(arguments).await,
            "socket.send" => self.sockets.send(arguments).await,
            "socket.receive" => self.sockets.receive(arguments).await,
            "socket.close" => self.sockets.close(arguments).await,
            "oauth.mcp.begin" => {
                let port = u16::try_from(arguments["port"].as_u64().unwrap_or_default()).unwrap_or_default();
                let started = self.sign_in(arguments).begin(self.http, port).await?;
                Ok(serde_json::to_value(started).unwrap_or_default())
            }
            "oauth.mcp.exchange" => {
                let authorization: Authorization = parsed(&arguments["authorization"], "authorization")?;
                let code = field(arguments, "code");
                let grant =
                    self.sign_in(arguments).exchange(self.http, &authorization, code, now_of(arguments)).await?;
                Ok(serde_json::to_value(grant).unwrap_or_default())
            }
            "oauth.mcp.refresh" => {
                let grant: Grant = parsed(&arguments["grant"], "grant")?;
                let fresh = self.sign_in(arguments).refresh(self.http, &grant, now_of(arguments)).await?;
                Ok(serde_json::to_value(fresh).unwrap_or_default())
            }
            "oauth.code.begin" => {
                let port = u16::try_from(arguments["port"].as_u64().unwrap_or_default()).unwrap_or_default();
                let started = self.code_sign_in(arguments).begin(self.http, port).await?;
                Ok(serde_json::to_value(started).unwrap_or_default())
            }
            "oauth.code.exchange" => {
                let authorization: Authorization = parsed(&arguments["authorization"], "authorization")?;
                let code = field(arguments, "code");
                let grant =
                    self.code_sign_in(arguments).exchange(self.http, &authorization, code, now_of(arguments)).await?;
                Ok(serde_json::to_value(grant).unwrap_or_default())
            }
            "oauth.code.refresh" => {
                let grant: Grant = parsed(&arguments["grant"], "grant")?;
                let fresh = self.code_sign_in(arguments).refresh(self.http, &grant, now_of(arguments)).await?;
                Ok(serde_json::to_value(fresh).unwrap_or_default())
            }
            other => Err(ActionError::permanent(format!("the app offers no `{other}`"))),
        }
    }
}

impl Waits for Online<'_> {
    async fn answer(&self, function: &str, arguments: Value) -> Result<Value, String> {
        self.reach(function, &arguments).await.map_err(|error| encoded(&error))
    }
}
