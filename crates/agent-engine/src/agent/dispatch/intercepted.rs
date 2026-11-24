use serde_json::{Value, json};

use super::Calling;
use crate::agent::Agent;
use crate::intercept::{Chain, Kind};
use crate::tools::Outcome;

impl Agent<'_> {
    pub(super) fn intercepted<'env>(&'env self, calling: &Calling<'_, 'env>, input: &Value) -> Outcome {
        let interceptors = &self.workspace.interceptors;
        if !interceptors.listens_to(Kind::Tool) {
            return self.ran(calling, input);
        }
        let mut last: Option<Outcome> = None;
        let mut terminal = |changed: Value| {
            if changed != *input
                && let Some(refusal) = calling.tool.and_then(|tool| self.invalid_input(calling.name, tool, &changed))
            {
                return Err(refusal);
            }
            let outcome = self.ran(calling, &changed);
            let answer = json!({ "content": outcome.content, "isError": outcome.is_error });
            last = Some(outcome);
            Ok(answer)
        };
        let chain = Chain {
            kind: Kind::Tool,
            subject: calling.name,
            workspace: self.workspace,
            model: Some((self.api, &self.request)),
            terminal: &mut terminal,
        };
        match interceptors.run(input.clone(), chain) {
            Ok(answer) => answered(&answer, last),
            Err(error) => Outcome::error(format!("Interceptor failed: {error}")),
        }
    }
}

fn answered(answer: &Value, last: Option<Outcome>) -> Outcome {
    let content = match &answer["content"] {
        Value::String(text) => text.clone(),
        Value::Null => {
            return last.unwrap_or_else(|| Outcome::error("Interceptor failed: it returned no `content`"));
        }
        other => other.to_string(),
    };
    let is_error = answer["isError"] == true;
    match last {
        Some(outcome) if outcome.content == content && outcome.is_error == is_error => outcome,
        _ if is_error => Outcome::error(content),
        _ => Outcome::ok(content),
    }
}
