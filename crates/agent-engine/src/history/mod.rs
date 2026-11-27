mod shelf;

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

pub use shelf::{Kept, Shelf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Place {
    pub name: String,
    pub path: PathBuf,
    pub holds: String,
    pub exists: bool,
}

impl Place {
    pub fn new(name: &str, path: PathBuf, holds: &str) -> Self {
        let exists = path.exists();
        Self { name: name.to_owned(), path, holds: holds.to_owned(), exists }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub cwd: Option<String>,
    pub since_ms: Option<u64>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Session {
    pub session: String,
    pub kept_in: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub updated_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    #[serde(flatten)]
    pub session: Session,
    pub role: &'static str,
    pub at_ms: Option<u64>,
    pub turn: Option<usize>,
    pub excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Turn {
    pub turn: usize,
    pub role: &'static str,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Read {
    #[serde(flatten)]
    pub session: Session,
    pub total: usize,
    pub turns: Vec<Turn>,
}

pub trait History: Send + Sync {
    fn places(&self) -> Vec<Place>;
    fn sessions(&self, query: &Query) -> Vec<Session>;
    fn search(&self, query: &Query) -> Vec<Hit>;
    fn read(&self, session: &str, from: usize, count: usize) -> Result<Read, String>;
}

#[derive(Clone)]
pub struct HostHistory(pub Arc<dyn History>);

impl std::fmt::Debug for HostHistory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostHistory")
    }
}
