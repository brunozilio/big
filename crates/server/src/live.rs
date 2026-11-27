use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
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
    kept: Arc<Mutex<Kept>>,
}

#[derive(Debug, Clone)]
pub struct Told {
    pub id: u64,
    pub owner: Option<String>,
    pub data: String,
}

#[derive(Debug)]
struct Kept {
    epoch: String,
    last: u64,
    recent: VecDeque<Told>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Missed {
    Nothing,
    These(Vec<Told>),
    Unknown,
}

pub type Allowed = Arc<dyn Fn(&Told) -> bool + Send + Sync>;

impl Default for Live {
    fn default() -> Self {
        let epoch = platform::crypto::random(6).map_or_else(|_| "0".to_owned(), |bytes| platform::crypto::hex(&bytes));
        Live {
            sender: broadcast::channel(KEPT).0,
            kept: Arc::new(Mutex::new(Kept { epoch, last: 0, recent: VecDeque::with_capacity(KEPT) })),
        }
    }
}

impl PartialEq for Told {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Told {}

impl Kept {
    fn mark(&self, id: u64) -> String {
        format!("{}:{id}", self.epoch)
    }

    fn since(&self, after: Option<&str>) -> Missed {
        let Some(after) = after.filter(|after| !after.is_empty()) else { return Missed::Nothing };
        let seen =
            after.split_once(':').filter(|(epoch, _)| *epoch == self.epoch).and_then(|(_, id)| id.parse::<u64>().ok());
        let Some(seen) = seen.filter(|seen| *seen <= self.last) else { return Missed::Unknown };
        let oldest = self.recent.front().map_or(self.last + 1, |told| told.id);
        if seen + 1 < oldest {
            return Missed::Unknown;
        }
        Missed::These(self.recent.iter().filter(|told| told.id > seen).cloned().collect())
    }
}

impl Live {
    fn kept(&self) -> MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn send(&self, owner: Option<String>, data: String) {
        let mut kept = self.kept();
        kept.last += 1;
        let told = Told { id: kept.last, owner, data };
        if kept.recent.len() == KEPT {
            kept.recent.pop_front();
        }
        kept.recent.push_back(told.clone());
        let _ = self.sender.send(told);
    }

    pub fn publish(&self, event: &Value) {
        self.send(None, event.to_string());
    }

    pub fn publish_for(&self, owner: &str, event: &Value) {
        self.send(Some(owner.to_owned()), event.to_string());
    }

    pub fn missed(&self, after: Option<&str>) -> Missed {
        self.kept().since(after)
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

    pub fn branding(&self, owner: &str, branding: Value) {
        self.publish_for(owner, &json!({ "type": "branding", "branding": branding }));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Told> {
        self.sender.subscribe()
    }

    pub fn response(&self, after: Option<&str>) -> Response {
        self.response_for(Arc::new(|_: &Told| true), after)
    }

    pub fn response_for(&self, allowed: Allowed, after: Option<&str>) -> Response {
        let (receiver, missed, epoch, now) = {
            let kept = self.kept();
            (self.sender.subscribe(), kept.since(after), kept.epoch.clone(), kept.last)
        };
        let mark = move |id: u64| format!("{epoch}:{id}");
        let replayed = match missed {
            Missed::Nothing => vec![(Some(now), json!({ "type": "hello" }).to_string())],
            Missed::Unknown => vec![(None, resync())],
            Missed::These(missed) => {
                missed.into_iter().filter(|told| allowed(told)).map(|told| (Some(told.id), told.data)).collect()
            }
        };
        let events = stream::iter(replayed).chain(following(receiver, allowed)).map(move |(id, data)| {
            let event = Event::default().data(data);
            Ok::<Event, Infallible>(match id {
                Some(id) => event.id(mark(id)),
                None => event,
            })
        });
        Sse::new(events).keep_alive(KeepAlive::new().interval(KEEP_ALIVE)).into_response()
    }

    pub fn mark_of(&self, id: u64) -> String {
        self.kept().mark(id)
    }
}

fn resync() -> String {
    json!({ "type": "resync" }).to_string()
}

fn item_owner(event: &Value) -> Option<String> {
    event["item"]["owner"].as_str().map(str::to_owned)
}

fn following(receiver: broadcast::Receiver<Told>, allowed: Allowed) -> impl Stream<Item = (Option<u64>, String)> {
    stream::unfold((receiver, allowed), |(mut receiver, allowed)| async move {
        loop {
            match receiver.recv().await {
                Ok(told) if allowed(&told) => return Some(((Some(told.id), told.data), (receiver, allowed))),
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    receiver = receiver.resubscribe();
                    return Some(((None, resync()), (receiver, allowed)));
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(missed: Missed) -> Vec<u64> {
        match missed {
            Missed::These(told) => told.iter().map(|told| told.id).collect(),
            other => panic!("expected events, got {other:?}"),
        }
    }

    #[test]
    fn a_reconnect_gets_what_it_missed_and_a_stranger_mark_asks_for_a_resync() {
        let live = Live::default();
        for n in 0..3 {
            live.publish(&json!({ "n": n }));
        }
        assert_eq!(live.missed(None), Missed::Nothing);
        assert_eq!(ids(live.missed(Some(&live.mark_of(1)))), vec![2, 3]);
        assert_eq!(ids(live.missed(Some(&live.mark_of(3)))), Vec::<u64>::new());
        assert_eq!(live.missed(Some("other:1")), Missed::Unknown);
        assert_eq!(live.missed(Some(&live.mark_of(9))), Missed::Unknown);
        assert_eq!(live.missed(Some("garbage")), Missed::Unknown);
    }

    #[test]
    fn a_mark_older_than_what_is_kept_asks_for_a_resync() {
        let live = Live::default();
        for n in 0..(KEPT + 5) {
            live.publish(&json!({ "n": n }));
        }
        assert_eq!(live.missed(Some(&live.mark_of(2))), Missed::Unknown);
        assert_eq!(ids(live.missed(Some(&live.mark_of(5)))).len(), KEPT);
    }
}
