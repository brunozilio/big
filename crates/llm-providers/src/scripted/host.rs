use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use domain::{Language, ModelOption};
use jiff::{SignedDuration, Timestamp};
use serde_json::{Value, json};

use crate::oauth::{self, Tokens};
use crate::{catalog, limits, web_search};

fn field<'a>(arguments: &'a Value, name: &str) -> &'a str {
    arguments[name].as_str().unwrap_or_default()
}

fn decoded(text: &str, from: &str) -> Result<Vec<u8>, String> {
    match from {
        "" | "text" => Ok(text.as_bytes().to_vec()),
        "base64" => STANDARD.decode(text).map_err(|error| error.to_string()),
        "base64url" => URL_SAFE_NO_PAD
            .decode(text.trim_end_matches('='))
            .or_else(|_| URL_SAFE.decode(text))
            .map_err(|error| error.to_string()),
        other => Err(format!("no encoding is called {other}")),
    }
}

fn convert(arguments: &Value) -> Result<Value, String> {
    let bytes = decoded(field(arguments, "data"), field(arguments, "from"))?;
    let converted = match field(arguments, "to") {
        "" | "text" => String::from_utf8(bytes).map_err(|error| error.to_string())?,
        "base64" => STANDARD.encode(bytes),
        "base64url" => URL_SAFE_NO_PAD.encode(bytes),
        other => return Err(format!("no encoding is called {other}")),
    };
    Ok(Value::String(converted))
}

fn parsed<T: serde::de::DeserializeOwned>(value: &Value) -> Option<T> {
    serde_json::from_value(value.clone()).ok()
}

fn moment(arguments: &Value, name: &str) -> Timestamp {
    field(arguments, name).parse().unwrap_or_else(|_| Timestamp::now())
}

fn standard(arguments: &Value) -> Result<Value, String> {
    let status = u16::try_from(arguments["status"].as_u64().unwrap_or_default()).unwrap_or_default();
    let previous: Option<Tokens> = parsed(&arguments["previous"]);
    let now = moment(arguments, "now");
    let tokens = oauth::standard(status, field(arguments, "body"), previous.as_ref(), now, |_| Ok(String::new()))
        .map_err(|error| super::bridge::encoded(&error))?;
    serde_json::to_value(tokens).map_err(|error| error.to_string())
}

fn later(arguments: &Value) -> Value {
    let at = moment(arguments, "at");
    let seconds = arguments["seconds"].as_i64().unwrap_or_default();
    json!(at.checked_add(SignedDuration::from_secs(seconds)).ok().map(|at| at.to_string()))
}

fn form(arguments: &Value) -> Value {
    let pairs: Vec<(String, String)> = parsed(arguments).unwrap_or_default();
    json!(url::form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish())
}

fn query(arguments: &Value) -> Value {
    let query = field(arguments, "query");
    let found: serde_json::Map<String, Value> =
        url::form_urlencoded::parse(query.as_bytes()).map(|(key, value)| (key.into_owned(), json!(value))).collect();
    Value::Object(found)
}

fn models(function: &str, arguments: &Value) -> Option<Result<Value, String>> {
    let model: Option<ModelOption> = parsed(&arguments["model"]);
    Some(Ok(match function {
        "models.effort" => {
            let mut body = arguments["body"].clone();
            let always = arguments["always"].as_bool().unwrap_or(false);
            if let Err(error) =
                crate::effort(field(arguments, "vendor"), &mut body, model.as_ref(), field(arguments, "effort"), always)
            {
                return Some(Err(super::bridge::encoded(&error)));
            }
            body
        }
        "models.reasons" => {
            json!(
                crate::vendor(field(arguments, "vendor")).is_some_and(|vendor| vendor.reasons(field(arguments, "id")))
            )
        }
        "models.outputLimit" => json!(crate::providers::output_limit(model.as_ref(), &arguments["body"])),
        "models.webSearch" => match field(arguments, "format") {
            "messages" => web_search::anthropic(field(arguments, "name")),
            "responses" => web_search::responses(),
            _ => Value::Null,
        },
        "models.toolsInConversation" => json!(crate::added_tools::openai(field(arguments, "name"))),
        _ => return None,
    }))
}

fn limit(function: &str, arguments: &Value) -> Option<Value> {
    let seconds = arguments["seconds"].as_i64().unwrap_or_default();
    Some(match function {
        "limits.span" => {
            let language = Language::named(field(arguments, "language")).unwrap_or_default();
            json!(limits::span(seconds, language))
        }
        "limits.instant" => json!(limits::instant(arguments.as_str().unwrap_or_default())),
        "limits.epoch" => json!(limits::epoch(seconds)),
        "limits.after" => json!(limits::after(seconds)),
        "limits.titled" => json!(limits::titled(arguments.as_str().unwrap_or_default())),
        "limits.lasting" => json!(parsed(&arguments["window"]).map(|window| limits::lasting(window, seconds))),
        "limits.monthly" => json!(parsed(arguments).map(limits::monthly)),
        _ => return None,
    })
}

const HOME_LIMIT: u64 = 64 * 1024;

fn home_file(arguments: &Value) -> Value {
    let wanted = std::path::Path::new(arguments.as_str().unwrap_or_default());
    let plain = wanted.components().all(|part| matches!(part, std::path::Component::Normal(_)));
    let Some(home) = std::env::home_dir().filter(|_| plain && !wanted.as_os_str().is_empty()) else { return Value::Null };
    let path = home.join(wanted);
    let small = path.metadata().is_ok_and(|meta| meta.is_file() && meta.len() <= HOME_LIMIT);
    if !small {
        return Value::Null;
    }
    std::fs::read_to_string(path).map_or(Value::Null, Value::String)
}

fn local(function: &str, arguments: &Value) -> Option<Result<Value, String>> {
    let shown = |error: local_models::LocalError| error.to_string();
    Some(match function {
        "local.endpoint" => local_models::endpoint().map(Value::String).map_err(shown),
        "local.models" => Ok(local_models::models()),
        "local.remove" => local_models::remove(arguments.as_str().unwrap_or_default()).map(|()| Value::Null).map_err(shown),
        "files.home" => Ok(home_file(arguments)),
        _ => return None,
    })
}

#[derive(Debug, Clone)]
pub struct Local {
    pub provider: &'static str,
}

impl Local {
    pub fn answer(&self, function: &str, arguments: &Value) -> Option<Result<Value, String>> {
        if let Some(answer) = models(function, arguments) {
            return Some(answer);
        }
        if let Some(answer) = limit(function, arguments) {
            return Some(Ok(answer));
        }
        if let Some(answer) = local(function, arguments) {
            return Some(answer);
        }
        Some(match function {
            "log" => {
                tracing::info!(target: "provider", provider = self.provider, "{}", arguments.as_str().unwrap_or_default());
                Ok(Value::Null)
            }
            "encoding.convert" => convert(arguments),
            "oauth.challenge" => Ok(json!(oauth::challenge(arguments.as_str().unwrap_or_default()))),
            "oauth.standard" => standard(arguments),
            "oauth.form" => Ok(form(arguments)),
            "oauth.query" => Ok(query(arguments)),
            "oauth.account" => Ok(json!(oauth::account_label(arguments.as_str().unwrap_or_default()))),
            "clock.now" => Ok(json!(Timestamp::now().to_string())),
            "clock.later" => Ok(later(arguments)),
            "crypto.uuid" => Ok(json!(uuid::Uuid::new_v4().to_string())),
            "catalog.model" => Ok(json!(catalog::model_of(
                self.provider,
                &arguments["item"],
                arguments["id"].as_str().unwrap_or("id"),
                arguments["window"].as_str().unwrap_or("max_input_tokens"),
            ))),
            "catalog.option" => Ok(json!(catalog::option_of(
                self.provider,
                field(arguments, "id"),
                field(arguments, "label").to_owned()
            ))),
            _ => return None,
        })
    }
}

impl script::Host for Local {
    fn call(&self, function: &str, arguments: Value) -> Result<Value, String> {
        self.answer(function, &arguments)
            .unwrap_or_else(|| Err(format!("`{function}` is offered only while signing in")))
    }
}
