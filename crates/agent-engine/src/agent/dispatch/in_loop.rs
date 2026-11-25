use serde_json::Value;

use super::Calling;
use crate::agent::Agent;
use crate::tools::{Outcome, Tool};

pub(in crate::agent) fn not_alongside(name: &str) -> Outcome {
    Outcome::error(format!(
        "{name} cannot start background work from a group of calls run side by side. Call it on its own."
    ))
}

impl Agent<'_> {
    pub(super) fn in_loop<'env>(&'env self, calling: &Calling<'_, 'env>, input: &Value) -> Outcome {
        let Calling { name, tool, .. } = *calling;
        let Some(tool) = tool else { return Outcome::error(format!("There is no tool named {name}.")) };
        self.tool_of_its_own(calling, tool, input)
    }

    fn tool_of_its_own<'env>(&'env self, calling: &Calling<'_, 'env>, tool: &dyn Tool, input: &Value) -> Outcome {
        let summary = tool.summary(input);
        match (self.project_skill(tool, input), tool.script()) {
            (Some(skill), _) => self.skill_call(calling, tool, skill, &summary, input),
            (None, Some(script)) => self.script_call(calling, script, &summary, input),
            (None, None) => {
                let shown = tool.shown(input, self.workspace);
                self.permitted(&calling.permitting(&summary, shown.as_deref()), tool, input)
            }
        }
    }
}
