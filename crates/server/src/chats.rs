use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use app::{Client, EngineEvent, RunControl, SessionId};
use domain::conversation::Entry;
use serde_json::Value;

use crate::error::{ApiError, ApiResult};
use crate::marks::now_ms;
use crate::session_view::{self, Shown};

const NOTIFY_GAP: Duration = Duration::from_millis(300);

pub type Notify = Arc<dyn Fn(&str) + Send + Sync>;

struct Going {
    control: RunControl,
    shown: Shown,
}

#[derive(Clone, Default)]
pub struct Chats {
    runs: Arc<Mutex<HashMap<String, Going>>>,
}

impl std::fmt::Debug for Chats {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Chats").finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct Message<'a> {
    pub session: &'a str,
    pub folder: PathBuf,
    pub text: &'a str,
    pub model: app::Model,
    pub effort: Option<app::Effort>,
    pub shown: Shown,
}

impl Chats {
    fn locked(&self) -> MutexGuard<'_, HashMap<String, Going>> {
        self.runs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn running(&self, session: &str) -> bool {
        self.locked().contains_key(session)
    }

    pub fn view(&self, session: &str) -> Option<(Vec<Value>, &'static str, String, String)> {
        let runs = self.locked();
        let going = runs.get(session)?;
        let pending = going.shown.transcript.ask().map(|pending| &pending.ask);
        let events = session_view::events(&going.shown, pending);
        let status = session_view::status(&going.shown.transcript, pending.is_some());
        Some((events, status, going.shown.mode.clone(), going.shown.model.clone()))
    }

    pub fn pending(&self, session: &str) -> Option<domain::AgentAsk> {
        self.locked().get(session).and_then(|going| going.shown.transcript.ask().map(|pending| pending.ask.clone()))
    }

    pub fn send(&self, client: &Client, message: Message<'_>, notify: Notify) -> ApiResult<()> {
        if self.running(message.session) {
            return Err(ApiError::busy("Esta conversa ainda está respondendo."));
        }
        let id = SessionId(message.session.to_owned());
        let model = message.model.clone();
        let mut session = client.session(message.folder.clone(), model).resume(id);
        if let Some(effort) = message.effort {
            session = session.effort(effort);
        }
        let run = session.send(message.text).map_err(ApiError::failed)?;
        let mut shown = message.shown;
        shown.model = message.model.id.clone();
        shown.transcript.push(Entry::User(message.text.to_owned()));
        shown.transcript.begin();
        shown.stamp(now_ms());
        self.locked().insert(message.session.to_owned(), Going { control: run.control(), shown });
        notify(message.session);
        let (chats, session) = (self.clone(), message.session.to_owned());
        let pumped =
            std::thread::Builder::new().name("web-chat".into()).spawn(move || chats.pump(run, &session, &notify));
        if let Err(error) = pumped {
            self.locked().remove(message.session);
            return Err(ApiError::failed(error));
        }
        Ok(())
    }

    fn pump(&self, run: app::Run, session: &str, notify: &Notify) {
        let mut told = Instant::now();
        for event in run {
            let streaming = matches!(event, EngineEvent::TextDelta(_) | EngineEvent::ThinkingDelta(_));
            if let Some(going) = self.locked().get_mut(session) {
                going.shown.transcript.apply(&event);
                going.shown.stamp(now_ms());
            }
            if !streaming || told.elapsed() >= NOTIFY_GAP {
                notify(session);
                told = Instant::now();
            }
        }
        self.locked().remove(session);
        notify(session);
    }

    pub fn answer(&self, session: &str, ask: &str, decision: domain::Decision) -> ApiResult<()> {
        let runs = self.locked();
        let going = runs.get(session).ok_or_else(|| ApiError::missing("Nada nesta conversa espera resposta."))?;
        going.control.answer(app::AskId(ask.to_owned()), decision).map_err(ApiError::failed)
    }

    pub fn stop(&self, session: &str) -> ApiResult<bool> {
        let runs = self.locked();
        match runs.get(session) {
            Some(going) => going.control.cancel().map(|()| true).map_err(ApiError::failed),
            None => Ok(false),
        }
    }
}
