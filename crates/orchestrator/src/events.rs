use domain::workflow::ItemId;
use domain::{AgentAsk, AskId};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OrchestratorEvent {
    ItemChanged { item: ItemId, version: u64 },
    ItemEnded { item: ItemId },
    Session { item: ItemId, step: String, session: String },
    Asked { item: ItemId, step: String, session: Option<String>, ask: AgentAsk },
    AskClosed { item: ItemId, ask: AskId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    Poke,
    Rerun,
    Blocked(u64),
}
