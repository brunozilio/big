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
    suggestions: Arc<Mutex<HashMap<String, String>>>,
}

struct Resumed {
    client: Client,
    folder: PathBuf,
    model: app::Model,
    agent: Option<domain::MainAgentSpec>,
    session: String,
}

impl Resumed {
    fn session(&self) -> app::Session {
        let id = SessionId(self.session.clone());
        self.client.session(self.folder.clone(), self.model.clone()).resume(id).agent(self.agent.clone())
    }
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
    pub agent: Option<domain::MainAgentSpec>,
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

    pub fn suggestion(&self, session: &str) -> Option<String> {
        self.suggestions.lock().unwrap_or_else(PoisonError::into_inner).get(session).cloned()
    }

    pub fn side(client: &Client, message: &Message<'_>) -> ApiResult<String> {
        let resumed = Resumed {
            client: client.clone(),
            folder: message.folder.clone(),
            model: message.model.clone(),
            agent: message.agent.clone(),
            session: message.session.to_owned(),
        };
        resumed.session().side(message.text).and_then(app::Run::side_answer).map_err(ApiError::failed)
    }

    pub fn pending(&self, session: &str) -> Option<domain::AgentAsk> {
        self.locked().get(session).and_then(|going| going.shown.transcript.ask().map(|pending| pending.ask.clone()))
    }

    pub fn send(&self, client: &Client, message: Message<'_>, notify: Notify) -> ApiResult<()> {
        if self.running(message.session) {
            return Err(ApiError::busy("Esta conversa ainda está respondendo."));
        }
        self.suggestions.lock().unwrap_or_else(PoisonError::into_inner).remove(message.session);
        let resumed = Resumed {
            client: client.clone(),
            folder: message.folder.clone(),
            model: message.model.clone(),
            agent: message.agent.clone(),
            session: message.session.to_owned(),
        };
        let mut session = resumed.session();
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
        let chats = self.clone();
        let pumped =
            std::thread::Builder::new().name("web-chat".into()).spawn(move || chats.pump(run, &resumed, &notify));
        if let Err(error) = pumped {
            self.locked().remove(message.session);
            return Err(ApiError::failed(error));
        }
        Ok(())
    }

    fn pump(&self, run: app::Run, resumed: &Resumed, notify: &Notify) {
        let session = resumed.session.as_str();
        let mut told = Instant::now();
        let mut completed = false;
        for event in run {
            completed |= matches!(event, EngineEvent::Completed { .. });
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
        if completed {
            self.suggest(resumed, notify);
        }
    }

    fn suggest(&self, resumed: &Resumed, notify: &Notify) {
        let answer = resumed.session().suggest().and_then(app::Run::chore_answer);
        let Ok(Some(domain::ChoreAnswer::Suggestion(text))) = answer else { return };
        if self.running(&resumed.session) {
            return;
        }
        self.suggestions.lock().unwrap_or_else(PoisonError::into_inner).insert(resumed.session.clone(), text);
        notify(&resumed.session);
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
