use serde::{Deserialize, Serialize};

use super::McpApproval;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum McpOrigin {
    Host,
    Project,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpState {
    Ready,
    Pending,
    Off,
}

impl McpState {
    pub fn of(origin: &McpOrigin, name: &str, approval: &McpApproval) -> Self {
        if *origin != McpOrigin::Project || approval.allows(name) {
            Self::Ready
        } else if approval.disabled.iter().any(|known| known == name) {
            Self::Off
        } else {
            Self::Pending
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub origin: McpOrigin,
    pub target: String,
    pub state: McpState,
}
