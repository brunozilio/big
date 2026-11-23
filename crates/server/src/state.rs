use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use serde_json::Value;
use tokio::sync::mpsc;

use crate::backend::Backend;
use crate::chats::Chats;
use crate::live::Live;
use crate::sign_in::Flows;
use crate::stamps::Stamps;

#[derive(Debug)]
pub struct Followed {
    pub seen: HashMap<String, String>,
    pub status: String,
    pub wanted: Instant,
}

impl Default for Followed {
    fn default() -> Self {
        Followed { seen: HashMap::new(), status: String::new(), wanted: Instant::now() }
    }
}

#[derive(Debug)]
pub struct Shared {
    pub backend: Backend,
    pub live: Live,
    pub stamps: Stamps,
    pub chats: Chats,
    pub flows: Flows,
    pub followed: Mutex<HashMap<String, Followed>>,
    pub answered: Mutex<HashMap<String, Vec<Value>>>,
    pub nudge: mpsc::UnboundedSender<String>,
    pub port: u16,
    pub bundle: &'static [page::Asset],
}

pub type Ctx = Arc<Shared>;

pub fn held<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    pub fn follow(&self, session: &str) {
        let mut followed = held(&self.followed);
        followed.entry(session.to_owned()).or_default().wanted = Instant::now();
        drop(followed);
        let _ = self.nudge.send(session.to_owned());
    }
}
