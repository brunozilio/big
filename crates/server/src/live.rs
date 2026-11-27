use std::convert::Infallible;
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::stream::{self, Stream, StreamExt};
use serde_json::{Value, json};
use tokio::sync::broadcast;

const KEPT: usize = 1024;
const KEEP_ALIVE: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
pub struct Live {
    sender: broadcast::Sender<Told>,
}

#[derive(Debug, Clone)]
pub struct Told {
    pub owner: Option<String>,
    pub data: String,
}

pub type Allowed = std::sync::Arc<dyn Fn(&Told) -> bool + Send + Sync>;

impl Default for Live {
    fn default() -> Self {
        Live { sender: broadcast::channel(KEPT).0 }
    }
}

impl Live {
    pub fn publish(&self, event: &Value) {
        let _ = self.sender.send(Told { owner: None, data: event.to_string() });
    }

    pub fn publish_for(&self, owner: &str, event: &Value) {
        let _ = self.sender.send(Told { owner: Some(owner.to_owned()), data: event.to_string() });
    }

    pub fn item(&self, item: Value) {
        let event = json!({ "type": "item", "item": item });
        match item_owner(&event) {
            Some(owner) => self.publish_for(&owner, &event),
            None => self.publish(&event),
        }
    }

    pub fn removed(&self, id: &str) {
        self.publish(&json!({ "type": "item.removed", "id": id }));
    }

    pub fn session_event(&self, session: &str, event: Value) {
        self.publish(&json!({ "type": "session", "session": session, "event": event }));
    }

    pub fn session_event_removed(&self, session: &str, id: &str) {
        self.publish(&json!({ "type": "session.event.removed", "session": session, "id": id }));
    }

    pub fn session_suggestion(&self, session: &str, suggestion: Option<&str>) {
        self.publish(&json!({ "type": "session.suggestion", "session": session, "suggestion": suggestion }));
    }

    pub fn session_work(&self, session: &str, work: Value) {
        self.publish(&json!({ "type": "session.work", "session": session, "work": work }));
    }

    pub fn session_status(&self, session: &str, status: &str) {
        self.publish(&json!({ "type": "session.status", "session": session, "status": status }));
    }

    pub fn connections(&self, owner: &str, connections: Value) {
        self.publish_for(owner, &json!({ "type": "connections", "connections": connections }));
    }

    pub fn branding(&self, branding: Value) {
        self.publish(&json!({ "type": "branding", "branding": branding }));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Told> {
        self.sender.subscribe()
    }

    pub fn response(&self) -> Response {
        self.response_for(std::sync::Arc::new(|_: &Told| true))
    }

    pub fn response_for(&self, allowed: Allowed) -> Response {
        let events =
            following(self.subscribe(), allowed).map(|data| Ok::<Event, Infallible>(Event::default().data(data)));
        Sse::new(events).keep_alive(KeepAlive::new().interval(KEEP_ALIVE)).into_response()
    }
}

fn item_owner(event: &Value) -> Option<String> {
    event["item"]["owner"].as_str().map(str::to_owned)
}

fn following(receiver: broadcast::Receiver<Told>, allowed: Allowed) -> impl Stream<Item = String> {
    stream::unfold((receiver, allowed), |(mut receiver, allowed)| async move {
        loop {
            match receiver.recv().await {
                Ok(told) if allowed(&told) => return Some((told.data, (receiver, allowed))),
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    receiver = receiver.resubscribe();
                    return Some((json!({ "type": "resync" }).to_string(), (receiver, allowed)));
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    })
}
