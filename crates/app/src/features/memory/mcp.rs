use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use domain::label::Reads;
use domain::mcp::{LocalMcp, McpEndpoint, RpcError, text_result};
use domain::memory::{MEMORY_SEARCH_TOOL, MEMORY_SERVER, MemorySearches, SearchAnswer};
use domain::permission::{MemoryAccess, Surface};
use domain::{Memory, MemoryKind};
use platform::sync::Locked;
use serde_json::{Value, json};

use super::Memories;
use super::gate::{Gate, access_of};
use super::reading::Noted;

const DEFAULT_LIMIT: usize = 8;
const MAX_LIMIT: usize = 25;

#[cfg(test)]
fn respond(message: &Value, search: impl Fn(&str, usize) -> String) -> Option<Value> {
    domain::mcp::respond(
        message,
        MEMORY_SERVER,
        || vec![tool_definition()],
        |name, arguments| call(name, arguments, search),
    )
}

fn tool_definition() -> Value {
    json!({
        "name": MEMORY_SEARCH_TOOL,
        "description": "Search this project's memory from earlier sessions: facts about the project and the user \
                        (semantic), how things are done here (procedural) and what happened before (episodic). \
                        Matches by meaning and by words; ask in any language. Memories already in the conversation \
                        are not returned again. Search once per topic: an empty answer is final.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "What to look for." },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "description": "Most results to return (default 8)." }
            },
            "required": ["query"]
        }
    })
}

fn call(name: &str, arguments: &Value, search: impl Fn(&str, usize) -> String) -> Result<Value, RpcError> {
    if name != MEMORY_SEARCH_TOOL {
        return Err((-32602, format!("unknown tool: {name}")));
    }
    let query = arguments.get("query").and_then(Value::as_str).map(str::trim).unwrap_or_default();
    if query.is_empty() {
        return Ok(text_result("Give a query to search for.", true));
    }
    let limit =
        arguments.get("limit").and_then(Value::as_u64).map_or(DEFAULT_LIMIT, |n| n as usize).clamp(1, MAX_LIMIT);
    Ok(text_result(search(query, limit), false))
}

pub fn answer(answer: SearchAnswer) -> String {
    const STOP: &str = "Do not search memory again for this; carry on with the task.";
    match answer {
        SearchAnswer::Found(memories) => listing(&memories),
        SearchAnswer::Nothing => format!("Nothing in this project's memory matches. {STOP}"),
        SearchAnswer::AlreadyShown => {
            format!("Every memory that matches is already in this conversation. {STOP}")
        }
        SearchAnswer::Exhausted => format!(
            "You have searched memory {} times this turn, the most a turn allows. {STOP}",
            MemorySearches::LIMIT
        ),
    }
}

fn listing(memories: &[Memory]) -> String {
    memories
        .iter()
        .map(|m| {
            let kind = match m.kind {
                MemoryKind::Episodic => "episodic",
                MemoryKind::Semantic => "semantic",
                MemoryKind::Procedural => "procedural",
            };
            let origin = m.origin.as_deref().map(|o| format!(" (from \"{o}\")")).unwrap_or_default();
            m.wrapped(format!("[{}] {kind}: {}{origin}", m.id.0, m.text))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub struct MemoryEndpoint {
    memories: Memories,
    project: PathBuf,
    searches: Mutex<MemorySearches>,
    writer: Option<String>,
    surface: Surface,
    reads: Option<Reads>,
}

impl MemoryEndpoint {
    pub fn for_run(
        memories: Memories,
        project: PathBuf,
        searches: MemorySearches,
        surface: Surface,
        reads: Reads,
    ) -> LocalMcp {
        let searches = Mutex::new(searches);
        let endpoint = Self { memories, project, searches, writer: None, surface, reads: Some(reads) };
        LocalMcp::new(MEMORY_SERVER, Arc::new(endpoint))
    }

    pub fn for_agent(memories: Memories, project: PathBuf, origin: String, reads: Option<Reads>) -> LocalMcp {
        let searches = Mutex::new(MemorySearches::default());
        let endpoint = Self { memories, project, searches, writer: Some(origin), surface: Surface::Mcp, reads };
        LocalMcp::new(MEMORY_SERVER, Arc::new(endpoint))
    }

    fn gate(&self, stage: &'static str) -> Gate<'_> {
        Gate { memories: &self.memories, project: &self.project, surface: self.surface, stage }
    }

    fn searched(&self, name: &str, arguments: &Value) -> Result<Value, RpcError> {
        let gate = self.gate("tools/call");
        let refused = match (name == MEMORY_SEARCH_TOOL, &self.writer) {
            (false, _) => None,
            (true, Some(_)) => gate.listed(MEMORY_SEARCH_TOOL, MemoryAccess::Read),
            (true, None) => gate.refused(MemoryAccess::Read),
        };
        match refused {
            Some(refused) => Ok(text_result(refused, true)),
            None => call(name, arguments, |query, limit| self.search(query, limit)),
        }
    }

    fn search(&self, query: &str, limit: usize) -> String {
        let mut searches = self.searches.locked();
        if self.writer.is_some() {
            *searches = MemorySearches::default();
        }
        let found = self.memories.find(&self.project, query, limit, &mut searches);
        if let SearchAnswer::Found(memories) = &found {
            Noted { surface: self.surface, stage: "tools/call" }.read(self.reads.as_ref(), memories);
        }
        answer(found)
    }
}

impl McpEndpoint for MemoryEndpoint {
    fn handle(&self, message: &Value) -> Option<Value> {
        let Some(origin) = &self.writer else {
            return domain::mcp::respond(message, MEMORY_SERVER, || vec![tool_definition()], |name, arguments| {
                self.searched(name, arguments)
            });
        };
        domain::mcp::respond(
            message,
            MEMORY_SERVER,
            || std::iter::once(tool_definition()).chain(write::tools()).collect(),
            |name, arguments| match write::edit_of(name, arguments) {
                None => self.searched(name, arguments),
                Some(Err(told)) => Ok(text_result(told, true)),
                Some(Ok(edit)) => match self.gate("tools/call").listed(name, access_of(&edit)) {
                    Some(refused) => Ok(text_result(refused, true)),
                    None => {
                        let by = write::Writer { origin, lineage: super::reading::lineage(self.reads.as_ref()) };
                        Ok(write::applied(&self.memories, &self.project, edit, by))
                    }
                },
            },
        )
    }
}

mod write;

#[cfg(test)]
mod tests;
