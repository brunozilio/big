use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scheduled {
    pub id: String,
    pub prompt: String,
    pub next: i64,
}

pub const MOST: usize = 50;
