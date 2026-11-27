use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use serde_json::{Value, json};

use super::bridge::{Script, encoded};
use super::host::Local;
use crate::sign_in::{Answer, Listener};
use crate::{Host, ProviderError};

const OFFERED: [&str; 13] = [
    "local.download",
    "http.post",
    "http.get",
    "browser.open",
    "crypto.random",
    "clock.sleep",
    "clock.now",
    "callback.listen",
    "callback.next",
    "callback.code",
    "callback.finish",
    "callback.redirect",
    "callback.signedIn",
];

struct Call {
    function: String,
    arguments: Value,
    reply: Sender<Result<Value, String>>,
}

struct Remote {
    calls: Sender<Call>,
    local: Local,
}

impl script::Host for Remote {
    fn call(&self, function: &str, arguments: Value) -> Result<Value, String> {
        if !OFFERED.contains(&function)
            && let Some(answer) = self.local.answer(function, &arguments)
        {
            return answer;
        }
        let (reply, answer) = channel();
        let call = Call { function: function.to_owned(), arguments, reply };
        self.calls.send(call).map_err(|_| "the sign-in stopped listening".to_owned())?;
        answer.recv().map_err(|_| "the sign-in stopped answering".to_owned())?
    }
}

#[derive(Default)]
struct Desk {
    listeners: Vec<Box<dyn Listener>>,
    answers: Vec<Option<Box<dyn Answer>>>,
}

fn text(arguments: &Value, name: &str) -> String {
    arguments[name].as_str().unwrap_or_default().to_owned()
}

fn headers(arguments: &Value) -> Vec<(String, String)> {
    serde_json::from_value(arguments["headers"].clone()).unwrap_or_default()
}

fn index(arguments: &Value, name: &str) -> usize {
    arguments[name].as_u64().and_then(|at| usize::try_from(at).ok()).unwrap_or(usize::MAX)
}

fn reached((status, body): (u16, String)) -> Value {
    json!({ "status": status, "body": body })
}

impl Desk {
    fn answer(&mut self, host: &dyn Host, function: &str, arguments: &Value) -> Result<Value, ProviderError> {
        Ok(match function {
            "http.post" => {
                reached(host.post(&text(arguments, "url"), &headers(arguments), &text(arguments, "body"))?)
            }
            "http.get" => reached(host.get(&text(arguments, "url"), &headers(arguments))?),
            "local.download" => host.download(arguments)?,
            "browser.open" => {
                host.open(text(arguments, "url"), arguments["code"].as_str().map(str::to_owned));
                Value::Null
            }
            "crypto.random" => json!(host.random(usize::try_from(arguments.as_u64().unwrap_or(32)).unwrap_or(32))?),
            "clock.sleep" => {
                host.sleep(Duration::from_secs(arguments.as_u64().unwrap_or(1)));
                Value::Null
            }
            "clock.now" => json!(host.now().to_string()),
            "callback.listen" => {
                let port = u16::try_from(arguments["port"].as_u64().unwrap_or_default()).unwrap_or_default();
                let listener = host.listen(port, &text(arguments, "path"))?;
                let shown = json!({ "listener": self.listeners.len(), "redirect": listener.redirect_uri(), "port": listener.port() });
                self.listeners.push(listener);
                shown
            }
            "callback.next" => self.next(arguments)?,
            _ => self.finish(function, arguments)?,
        })
    }

    fn next(&mut self, arguments: &Value) -> Result<Value, ProviderError> {
        let listener = self
            .listeners
            .get(index(arguments, "listener"))
            .ok_or_else(|| ProviderError::Invalid("no such callback listener".into()))?;
        let others: Vec<String> = serde_json::from_value(arguments["others"].clone()).unwrap_or_default();
        let others: Vec<&str> = others.iter().map(String::as_str).collect();
        let key = arguments["key"].as_str().filter(|key| !key.is_empty()).unwrap_or("state");
        let answer = listener.next_by(key, &text(arguments, "state"), &others)?;
        let shown = json!({ "answer": self.answers.len(), "path": answer.path(), "query": answer.query() });
        self.answers.push(Some(answer));
        Ok(shown)
    }

    fn finish(&mut self, function: &str, arguments: &Value) -> Result<Value, ProviderError> {
        let slot = self.answers.get_mut(index(arguments, "answer"));
        let gone = || ProviderError::Invalid("the callback was already answered".into());
        if function == "callback.code" {
            let answer = slot.and_then(|answer| answer.as_ref()).ok_or_else(gone)?;
            return answer.code().map(Value::String);
        }
        let answer = slot.and_then(Option::take).ok_or_else(gone)?;
        match function {
            "callback.redirect" => answer.redirect(&text(arguments, "to")),
            "callback.signedIn" => answer.signed_in(arguments["account"].as_str().map(str::to_owned)),
            "callback.finish" => {
                answer.finish(arguments["error"].as_str().map_or(Ok(()), |error| Err(error.to_owned())))
            }
            other => return Err(ProviderError::Invalid(format!("no sign-in step is called {other}"))),
        }
        Ok(Value::Null)
    }
}

pub fn run(script: &Script, local: Local, host: &dyn Host) -> Result<Value, ProviderError> {
    let (calls, requests) = channel::<Call>();
    let remote: Arc<dyn script::Host> = Arc::new(Remote { calls, local });
    let script = script.clone();
    let input = json!({ "fields": host.fields() });
    let worker = std::thread::spawn(move || script.invoke("signIn", input, remote));
    let mut desk = Desk::default();
    while let Ok(call) = requests.recv() {
        let answer = desk.answer(host, &call.function, &call.arguments).map_err(|error| encoded(&error));
        let _ = call.reply.send(answer);
    }
    worker.join().map_err(|_| ProviderError::Invalid("the sign-in script stopped".into()))?
}
