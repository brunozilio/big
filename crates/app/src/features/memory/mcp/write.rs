use std::path::Path;

use domain::mcp::text_result;
use domain::{Author, MemoryEdit, MemoryId, Provenance};
use serde_json::{Map, Value, json};

use super::super::gate::REMEMBER;
use super::super::{Memories, now};

const REVISE: &str = "memory_revise";
const FORGET: &str = "memory_forget";

fn id_schema() -> Value {
    json!({ "type": "integer", "minimum": 0, "description": "The id shown in brackets by memory_search." })
}

pub(super) fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": REMEMBER,
            "description": "Keep something worth knowing in later sessions of this project: a fact about the project \
                            or the user (semantic), how things are done here (procedural) or what happened (episodic). \
                            One idea per call, in a sentence that stands on its own.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "What to remember." },
                    "kind": { "type": "string", "enum": ["semantic", "procedural", "episodic"] },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "scope": {
                        "type": "string",
                        "enum": ["repository", "application", "global"],
                        "description": "Where it applies: this repository (default), its whole application, or every project."
                    }
                },
                "required": ["text", "kind"]
            }
        }),
        json!({
            "name": REVISE,
            "description": "Reword a memory that is wrong or out of date.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": id_schema(), "text": { "type": "string", "description": "The new wording." } },
                "required": ["id", "text"]
            }
        }),
        json!({
            "name": FORGET,
            "description": "Forget a memory that is no longer true. The person can restore it in the app.",
            "inputSchema": { "type": "object", "properties": { "id": id_schema() }, "required": ["id"] }
        }),
    ]
}

pub(super) fn edit_of(name: &str, arguments: &Value) -> Option<Result<MemoryEdit, String>> {
    let op = match name {
        REMEMBER => "remember",
        REVISE => "revise",
        FORGET => "forget",
        _ => return None,
    };
    let mut fields = arguments.as_object().cloned().unwrap_or_else(Map::new);
    fields.insert("op".into(), json!(op));
    if op == "remember" {
        fields.entry("tags").or_insert_with(|| json!([]));
    }
    Some(
        serde_json::from_value(Value::Object(fields))
            .map_err(|error| format!("{name} could not read its arguments: {error}")),
    )
}

fn target(edit: &MemoryEdit) -> Option<MemoryId> {
    match edit {
        MemoryEdit::Forget { id } | MemoryEdit::Revise { id, .. } => Some(*id),
        _ => None,
    }
}

pub(super) fn applied(memories: &Memories, project: &Path, edit: MemoryEdit, origin: &str) -> Value {
    let known = memories.load(project).memories;
    if let Some(id) = target(&edit).filter(|id| !known.iter().any(|memory| memory.id == *id)) {
        return text_result(format!("There is no memory [{}] in this project; search for it first.", id.0), true);
    }
    let by = Provenance { author: Author::Agent, now: now(), origin: Some(origin.to_owned()) };
    let listing = match memories.edit(project, std::slice::from_ref(&edit), by) {
        Ok(listing) => listing,
        Err(error) => return text_result(error, true),
    };
    let told = match &edit {
        MemoryEdit::Remember { text, .. } => {
            let text = domain::memory::sanitize(text, memories.redactor()).into_inner();
            let kept = listing.memories.iter().find(|memory| memory.text == text);
            kept.map_or_else(|| "Remembered.".to_owned(), |memory| format!("Remembered as [{}].", memory.id.0))
        }
        MemoryEdit::Revise { id, .. } => format!("Revised [{}].", id.0),
        MemoryEdit::Forget { id } => format!("Forgot [{}].", id.0),
        MemoryEdit::Reclassify { id, .. } | MemoryEdit::Restore { id } => format!("Changed [{}].", id.0),
    };
    text_result(told, false)
}
