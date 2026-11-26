use minijinja::{Environment, Error, ErrorKind, Value as Jinja};
use serde_json::{Map, Value, json};

fn raise(message: String) -> Result<Jinja, Error> {
    Err(Error::new(ErrorKind::InvalidOperation, message))
}

fn now(format: String) -> String {
    jiff::Zoned::now().strftime(&format).to_string()
}

fn environment() -> Environment<'static> {
    let mut environment = Environment::new();
    minijinja_contrib::add_to_environment(&mut environment);
    environment.set_unknown_method_callback(minijinja_contrib::pycompat::unknown_method_callback);
    environment.add_function("raise_exception", raise);
    environment.add_function("strftime_now", now);
    environment
}

fn text_of(content: &Value) -> Value {
    match content {
        Value::Array(parts) => {
            let joined: Vec<&str> = parts.iter().filter_map(|part| part["text"].as_str()).collect();
            json!(joined.join("\n"))
        }
        other => other.clone(),
    }
}

fn call_of(call: &Value) -> Value {
    let mut call = call.clone();
    if let Some(arguments) = call["function"]["arguments"].as_str() {
        let parsed: Value = serde_json::from_str(arguments).unwrap_or_else(|_| Value::Object(Map::new()));
        call["function"]["arguments"] = parsed;
    }
    call
}

pub fn messages(sent: &Value) -> Value {
    let messages = sent.as_array().into_iter().flatten().map(|message| {
        let mut message = message.clone();
        message["content"] = text_of(&message["content"]);
        if let Some(calls) = message["tool_calls"].as_array() {
            message["tool_calls"] = calls.iter().map(call_of).collect();
        }
        message
    });
    Value::Array(messages.collect())
}

#[derive(Debug, Clone)]
pub struct Rendering<'a> {
    pub template: &'a str,
    pub messages: &'a Value,
    pub tools: &'a Value,
    pub extra: &'a Value,
    pub bos: &'a str,
    pub eos: &'a str,
}

pub fn render(rendering: &Rendering<'_>) -> Result<String, String> {
    let environment = environment();
    let template = environment.template_from_str(rendering.template).map_err(|error| error.to_string())?;
    let mut context = Map::new();
    context.insert("messages".into(), messages(rendering.messages));
    if rendering.tools.as_array().is_some_and(|tools| !tools.is_empty()) {
        context.insert("tools".into(), rendering.tools.clone());
    }
    context.insert("add_generation_prompt".into(), json!(true));
    context.insert("bos_token".into(), json!(rendering.bos));
    context.insert("eos_token".into(), json!(rendering.eos));
    for (key, value) in rendering.extra.as_object().into_iter().flatten() {
        context.insert(key.clone(), value.clone());
    }
    template.render(Value::Object(context)).map_err(|error| format!("{error:#}"))
}

#[cfg(test)]
#[path = "template_tests.rs"]
mod tests;
