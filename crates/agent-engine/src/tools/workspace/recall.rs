use std::collections::HashSet;

use serde_json::Value;

use super::Workspace;
use crate::transcript::{Block, Message};

const PATH_OF: [(&str, &str); 4] =
    [("Read", "file_path"), ("Write", "file_path"), ("Edit", "file_path"), ("NotebookEdit", "notebook_path")];

fn succeeded(history: &[Message]) -> HashSet<&str> {
    history
        .iter()
        .flat_map(|message| &message.blocks)
        .filter_map(|block| match block {
            Block::ToolResult { id, is_error: false, .. } | Block::ToolResultRich { id, is_error: false, .. } => {
                Some(id.as_str())
            }
            _ => None,
        })
        .collect()
}

fn sliced(name: &str, input: &Value) -> bool {
    name == "Read" && ["offset", "limit", "pages"].iter().any(|key| input.get(key).is_some_and(|v| !v.is_null()))
}

impl Workspace {
    pub fn recall_reads(&self, history: &[Message]) {
        let done = succeeded(history);
        for block in history.iter().flat_map(|message| &message.blocks) {
            let Block::ToolUse { id, name, input } = block else { continue };
            let Some((_, key)) = PATH_OF.iter().find(|(tool, _)| tool == name) else { continue };
            let Some(path) = input.get(*key).and_then(Value::as_str).filter(|path| !path.is_empty()) else {
                continue;
            };
            if !done.contains(id.as_str()) {
                continue;
            }
            let path = self.resolve(path);
            if sliced(name, input) {
                self.mark_partly_read(&path);
            } else {
                self.mark_read(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::tools::read_files::Freshness;
    use crate::transcript::Role;

    fn call(id: &str, name: &str, input: Value) -> Message {
        Message { role: Role::Assistant, blocks: vec![Block::ToolUse { id: id.into(), name: name.into(), input }] }
    }

    fn result(id: &str, is_error: bool) -> Message {
        Message { role: Role::User, blocks: vec![Block::ToolResult { id: id.into(), content: "ok".into(), is_error }] }
    }

    #[test]
    fn a_continued_conversation_remembers_the_files_it_read_or_wrote() {
        let dir = tempfile::tempdir().expect("dir");
        for name in ["read.rs", "edited.rs", "failed.rs", "slice.rs", "untouched.rs"] {
            std::fs::write(dir.path().join(name), "one\ntwo\n").expect("write");
        }
        let history = vec![
            call("r", "Read", json!({ "file_path": "read.rs" })),
            result("r", false),
            call(
                "e",
                "Edit",
                json!({ "file_path": dir.path().join("edited.rs"), "old_string": "a", "new_string": "" }),
            ),
            result("e", false),
            call("f", "Read", json!({ "file_path": "failed.rs" })),
            result("f", true),
            call("s", "Read", json!({ "file_path": "slice.rs", "offset": 2 })),
            result("s", false),
        ];
        let workspace = Workspace::new(dir.path());

        workspace.recall_reads(&history);

        let freshness = |name: &str| workspace.freshness(&dir.path().join(name));
        assert_eq!(freshness("read.rs"), Freshness::Current);
        assert_eq!(freshness("edited.rs"), Freshness::Current);
        assert_eq!(freshness("failed.rs"), Freshness::Unread, "a failed read never saw the file");
        assert_eq!(freshness("slice.rs"), Freshness::Partial);
        assert_eq!(freshness("untouched.rs"), Freshness::Unread);

        std::fs::write(dir.path().join("read.rs"), "changed by someone else, much longer\n").expect("write");
        assert_eq!(freshness("read.rs"), Freshness::Changed, "a change after the run started is still seen");
    }
}
