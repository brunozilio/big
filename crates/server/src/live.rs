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
    sender: broadcast::Sender<String>,
}

impl Default for Live {
    fn default() -> Self {
        Live { sender: broadcast::channel(KEPT).0 }
    }
}

impl Live {
    pub fn publish(&self, event: &Value) {
        let _ = self.sender.send(event.to_string());
    }

    pub fn item(&self, item: Value) {
        self.publish(&json!({ "type": "item", "item": item }));
    }

    pub fn removed(&self, id: &str) {
        self.publish(&json!({ "type": "item.removed", "id": id }));
    }

    pub fn session_event(&self, session: &str, event: Value) {
        self.publish(&json!({ "type": "session", "session": session, "event": event }));
    }

    pub fn session_suggestion(&self, session: &str, suggestion: Option<&str>) {
        self.publish(&json!({ "type": "session.suggestion", "session": session, "suggestion": suggestion }));
    }

    pub fn session_status(&self, session: &str, status: &str) {
        self.publish(&json!({ "type": "session.status", "session": session, "status": status }));
    }

    pub fn connections(&self, connections: Value) {
        self.publish(&json!({ "type": "connections", "connections": connections }));
    }

    pub fn branding(&self, branding: Value) {
        self.publish(&json!({ "type": "branding", "branding": branding }));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.sender.subscribe()
    }

    pub fn response(&self) -> Response {
        let events = following(self.subscribe()).map(|data| Ok::<Event, Infallible>(Event::default().data(data)));
        Sse::new(events).keep_alive(KeepAlive::new().interval(KEEP_ALIVE)).into_response()
    }
}

fn following(receiver: broadcast::Receiver<String>) -> impl Stream<Item = String> {
    stream::unfold(receiver, |mut receiver| async move {
        match receiver.recv().await {
            Ok(data) => Some((data, receiver)),
            Err(broadcast::error::RecvError::Lagged(_)) => {
                receiver = receiver.resubscribe();
                Some((json!({ "type": "resync" }).to_string(), receiver))
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    })
}
