use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use jiff::Timestamp;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::bridge::{Answers, Waits, encoded};
use crate::error::ActionError;
use crate::http::{Http, HttpRequest, HttpResponse, Method};
use crate::mcp::McpClient;
use crate::oauth::mcp::McpSignIn;
use crate::oauth::{Authorization, Grant, LoopbackSignIn};

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
            "encoding.convert" => convert(arguments),
            "clock.now" => Ok(json!(Timestamp::now().as_second())),
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
    }
    Ok(request)
}

fn response(answer: &HttpResponse, binary: bool) -> Value {
    let headers: Map<String, Value> =
        answer.headers.iter().map(|(name, value)| (name.to_ascii_lowercase(), json!(value))).collect();
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
    pub http: &'a dyn Http,
    pub redirect: String,
}

impl Online<'_> {
    fn sign_in(&self, arguments: &Value) -> McpSignIn {
        let scopes = arguments["scopes"].as_array().into_iter().flatten().filter_map(Value::as_str);
        let redirect = arguments["redirect"].as_str().filter(|redirect| !redirect.is_empty());
        McpSignIn {
            server: field(arguments, "server").to_owned(),
            redirect: redirect.map_or_else(|| self.redirect.clone(), str::to_owned),
            client_name: brand::NAME.to_owned(),
            scopes: scopes.map(str::to_owned).collect(),
        }
    }

    async fn reach(&self, function: &str, arguments: &Value) -> Result<Value, ActionError> {
        let token = field(arguments, "token");
        match function {
            "http" => match self.http.send(request(arguments)?).await {
                Ok(answer) => Ok(response(&answer, arguments["binary"].as_bool().unwrap_or(false))),
                Err(error) => Ok(json!({ "failed": error })),
            },
            "mcp.tools" => {
                Ok(json!(McpClient::connect(self.http, field(arguments, "url"), token).await?.tools().await?))
            }
            "mcp.call" => {
                let client = McpClient::connect(self.http, field(arguments, "url"), token).await?;
                client.call(field(arguments, "tool"), arguments["arguments"].clone()).await
            }
            "mcp.reachable" => Ok(json!(McpClient::connect(self.http, field(arguments, "url"), token).await.is_ok())),
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
            other => Err(ActionError::permanent(format!("the app offers no `{other}`"))),
        }
    }
}

impl Waits for Online<'_> {
    async fn answer(&self, function: &str, arguments: Value) -> Result<Value, String> {
        self.reach(function, &arguments).await.map_err(|error| encoded(&error))
    }
}
