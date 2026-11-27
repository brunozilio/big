use super::log::{self, Entry};
use crate::transcript::{Message, Role};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Told {
    pub turns: Vec<Said>,
    pub prompts: Vec<Prompt>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub user: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub at_ms: u64,
    pub text: String,
}

impl Told {
    pub fn add_log(&mut self, text: &str) {
        let (entries, _) = log::lines(text);
        for entry in &entries {
            match entry {
                Entry::Prompted { at_ms, prompt } => {
                    self.prompts.push(Prompt { at_ms: *at_ms, text: prompt.clone() });
                }
                Entry::Opened { run, .. } if self.cwd.is_none() && !run.cwd.is_empty() => {
                    self.cwd = Some(run.cwd.clone());
                }
                _ => {}
            }
        }
        let history = log::conversation(entries);
        if !history.is_empty() {
            self.set_turns(&history);
        }
    }

    pub fn add_saved(&mut self, text: &str) {
        if !self.turns.is_empty() {
            return;
        }
        if let Ok(history) = serde_json::from_str::<Vec<Message>>(text) {
            self.set_turns(&history);
        }
    }

    fn set_turns(&mut self, history: &[Message]) {
        self.turns = history
            .iter()
            .map(|message| Said { user: message.role == Role::User, text: message.text() })
            .filter(|said| !said.text.trim().is_empty())
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(json: serde_json::Value) -> String {
        format!("{json}\n")
    }

    #[test]
    fn a_log_tells_the_prompts_with_their_time_the_folder_and_the_conversation() {
        let text = [
            line(serde_json::json!({"kind": "prompted", "at_ms": 7, "prompt": "find the gateway", "seq": 1})),
            line(serde_json::json!({"type": "trace", "kind": "opened", "at_ms": 8, "run": {
                "model": "m", "provider": "p", "permission": "x", "effort": "low", "cwd": "/work/api"
            }, "seq": 2})),
            line(serde_json::json!({"kind": "message", "role": "User", "blocks": [{"Text": "find the gateway"}], "seq": 3})),
            line(serde_json::json!({"kind": "message", "role": "Assistant", "blocks": [
                {"ToolUse": {"id": "t", "name": "Read", "input": {}}}
            ], "seq": 4})),
            line(serde_json::json!({"kind": "message", "role": "Assistant", "blocks": [{"Text": "it is in main.go"}], "seq": 5})),
        ]
        .concat();
        let mut told = Told::default();
        told.add_log(&text);
        assert_eq!(told.prompts, vec![Prompt { at_ms: 7, text: "find the gateway".into() }]);
        assert_eq!(told.cwd.as_deref(), Some("/work/api"));
        assert_eq!(
            told.turns,
            vec![
                Said { user: true, text: "find the gateway".into() },
                Said { user: false, text: "it is in main.go".into() },
            ],
            "a turn with no text is left out"
        );
    }

    #[test]
    fn a_conversation_saved_whole_tells_its_turns_unless_the_log_already_did() {
        let saved = serde_json::to_string(&vec![Message::user("old question")]).unwrap_or_default();
        let mut told = Told::default();
        told.add_saved(&saved);
        assert_eq!(told.turns, vec![Said { user: true, text: "old question".into() }]);
        told.add_saved(&serde_json::to_string(&vec![Message::user("other")]).unwrap_or_default());
        assert_eq!(told.turns.len(), 1);
        told.add_saved("not json");
        assert_eq!(told.turns.len(), 1);
    }
}
