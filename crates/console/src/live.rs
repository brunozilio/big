use std::collections::BTreeMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::{StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::stream::{self, Stream, StreamExt};
use serde_json::Value;
use tokio::sync::{OwnedSemaphorePermit, broadcast};

use crate::chats::Desk;
use crate::routes::{Shared, failure};

const KEPT_UPDATES: usize = 256;
pub const MOST_SUBSCRIBERS: usize = 32;
pub const MOST_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Default)]
struct Board {
    sessions: Option<String>,
    latest: BTreeMap<String, String>,
    watched: Option<String>,
}

#[derive(Debug, Clone)]
enum Update {
    Session(String),
    Chat(String),
    Resync,
}

impl Update {
    fn event(self) -> Event {
        match self {
            Self::Session(line) => Event::default().data(line),
            Self::Chat(session) => Event::default().event("chat").data(session),
            Self::Resync => Event::default().event("resync").data("missed"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Live {
    board: Arc<Mutex<Board>>,
    updates: broadcast::Sender<Update>,
    desk: Desk,
}

impl Default for Live {
    fn default() -> Self {
        Self { board: Arc::default(), updates: broadcast::channel(KEPT_UPDATES).0, desk: Desk::default() }
    }
}

impl Live {
    fn board(&self) -> MutexGuard<'_, Board> {
        self.board.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn desk(&self) -> &Desk {
        &self.desk
    }

    pub fn chat_changed(&self, session: &str) {
        let _ = self.updates.send(Update::Chat(session.to_owned()));
    }

    pub fn list(&self, sessions: String) {
        self.board().sessions = Some(sessions);
    }

    pub fn send(&self, session: &str, data: &str) {
        let line = data.replace(['\r', '\n'], " ");
        self.board().latest.insert(session.to_owned(), line.clone());
        let _ = self.updates.send(Update::Session(line));
    }

    pub fn watched(&self) -> Option<String> {
        self.board().watched.clone()
    }

    pub fn folder_of(&self, session: &str) -> Option<PathBuf> {
        let listed: Value = serde_json::from_str(self.board().sessions.as_deref()?).ok()?;
        let card = listed.as_array()?.iter().find(|card| card["session"].as_str() == Some(session))?;
        Some(PathBuf::from(card["cwd"].as_str()?)).filter(|folder| folder.is_dir())
    }
}

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/sessions", get(sessions))
        .route("/api/latest", get(latest))
        .route("/api/events", get(events))
        .route("/api/watch", post(watch))
}

fn json(body: String) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

async fn sessions(State(shared): State<Shared>) -> Response {
    json(shared.live.board().sessions.clone().unwrap_or_else(|| "[]".into()))
}

async fn latest(State(shared): State<Shared>) -> Response {
    let lines: Vec<String> = shared.live.board().latest.values().cloned().collect();
    json(format!("[{}]", lines.join(",")))
}

struct Following {
    receiver: broadcast::Receiver<Update>,
    _place: OwnedSemaphorePermit,
}

fn following(receiver: broadcast::Receiver<Update>, place: OwnedSemaphorePermit) -> impl Stream<Item = Update> {
    stream::unfold(Following { receiver, _place: place }, |mut state| async move {
        match state.receiver.recv().await {
            Ok(update) => Some((update, state)),
            Err(broadcast::error::RecvError::Lagged(_)) => {
                state.receiver = state.receiver.resubscribe();
                Some((Update::Resync, state))
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    })
}

async fn events(State(shared): State<Shared>) -> Response {
    let Ok(place) = shared.subscribers.clone().try_acquire_owned() else {
        let told = format!("{MOST_SUBSCRIBERS} pages are already following this console; close one first.");
        return failure(StatusCode::TOO_MANY_REQUESTS, "too_many_subscribers", &told);
    };
    let receiver = shared.live.updates.subscribe();
    let latest: Vec<Update> = shared.live.board().latest.values().cloned().map(Update::Session).collect();
    let updates = stream::iter(latest).chain(following(receiver, place));
    let events = updates.map(|update| Ok::<Event, Infallible>(update.event()));
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))).into_response()
}

fn named(body: &Value, field: &str) -> Option<String> {
    body[field].as_str().map(str::trim).filter(|text| !text.is_empty()).map(str::to_owned)
}

fn unreadable(rejection: &JsonRejection) -> Response {
    if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
        let told = format!("A request to this console may carry at most {MOST_BODY_BYTES} bytes.");
        return failure(StatusCode::PAYLOAD_TOO_LARGE, "too_large", &told);
    }
    failure(StatusCode::BAD_REQUEST, "validation", "The request's body is not the JSON this console expects.")
}

async fn watch(State(shared): State<Shared>, body: Result<axum::Json<Value>, JsonRejection>) -> Response {
    let body = match body {
        Ok(axum::Json(body)) => body,
        Err(rejection) => return unreadable(&rejection),
    };
    let Some(session) = named(&body, "session") else {
        return failure(StatusCode::BAD_REQUEST, "validation", "Say which session to watch.");
    };
    let Some(session) = crate::chats::session_id(&session) else {
        return failure(StatusCode::BAD_REQUEST, "validation", "That is not a session id.");
    };
    shared.live.board().watched = Some(session.to_owned());
    StatusCode::ACCEPTED.into_response()
}
