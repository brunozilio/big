use std::path::PathBuf;

use domain::memory::{Forgotten, Memory};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryListing {
    pub path: PathBuf,
    pub memories: Vec<Memory>,
    pub forgotten: Vec<Forgotten>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryApplied {
    pub added: usize,
    pub changed: usize,
    pub forgotten: usize,
    pub listing: MemoryListing,
}
