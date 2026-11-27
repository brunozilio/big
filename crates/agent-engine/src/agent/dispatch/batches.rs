use domain::{EngineEvent, ToolCall};
use platform::sync::Locked;
use serde_json::Value;

use crate::agent::Agent;
use crate::agent::run::ThisRun;
use crate::agent::subagents::background::Spawn;
use crate::tools::names::ToolName;
use crate::transcript::{Block, Message};

const SIDE_BY_SIDE: usize = 8;
const AGENTS_AT_ONCE: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lane {
    Alone,
    SideBySide,
    Agents,
}

impl Agent<'_> {
    pub(in crate::agent) fn pending_rules(&self, calls: &[(String, String, Value)]) -> Vec<String> {
        let mut new_rules = Vec::new();
        let mut seen = self.workspace.tracking.seen_rules.locked();
        for (_, _, input) in calls {
            let path =
                input["file_path"].as_str().or_else(|| input["path"].as_str()).map(|p| self.workspace.resolve(p));
            for rule in &self.workspace.rules {
                match rule.applies(&self.workspace.cwd, path.as_deref()) {
                    Ok(true) if !rule.patterns.is_empty() && seen.insert(rule.path.clone()) => {
                        new_rules.push(format!("{}:\n{}", rule.path.display(), rule.text))
                    }
                    Err(error) if rule.error.is_none() || seen.insert(rule.path.clone()) => {
                        tracing::warn!(file = %rule.path.display(), %error, "skipping a rule file that could not be read as a rule");
                    }
                    _ => {}
                }
            }
        }
        new_rules
    }

    pub(in crate::agent) fn results<'env>(
        &'env self,
        calls: &[(String, String, Value)],
        new_rules: &[String],
        run: &ThisRun,
        history: &[Message],
        spawn: Spawn<'_, 'env>,
    ) -> Vec<Block> {
        if !new_rules.is_empty() {
            return self.sent_back_to_read(calls, new_rules);
        }
        let lanes: Vec<Lane> = calls.iter().map(|(_, name, input)| self.lane(run, name, input)).collect();
        let mut results = Vec::with_capacity(calls.len());
        for range in batches(&lanes) {
            let group = &calls[range.clone()];
            match lanes[range.start] {
                Lane::SideBySide if group.len() > 1 => {
                    group.chunks(SIDE_BY_SIDE).for_each(|chunk| results.extend(self.side_by_side(chunk, run)));
                }
                Lane::Agents if group.len() > 1 => {
                    for chunk in group.chunks(AGENTS_AT_ONCE) {
                        results.extend(self.agents_at_once(chunk, run, history, spawn));
                    }
                }
                _ => results.extend(group.iter().map(|(id, name, input)| self.call(id, name, input, run, history, spawn))),
            }
        }
        results
    }

    fn lane(&self, run: &ThisRun, name: &str, input: &Value) -> Lane {
        let teammate = input.get("name").and_then(Value::as_str).is_some_and(|name| !name.trim().is_empty());
        if self.concurrent(run, name, input) {
            Lane::SideBySide
        } else if self.depth == 0 && ToolName::of(name) == ToolName::Agent && !teammate {
            Lane::Agents
        } else {
            Lane::Alone
        }
    }

    fn agents_at_once<'env>(
        &'env self,
        chunk: &[(String, String, Value)],
        run: &ThisRun,
        history: &[Message],
        spawn: Spawn<'_, 'env>,
    ) -> Vec<Block> {
        std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|(id, name, input)| scope.spawn(move || self.call(id, name, input, run, history, spawn)))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)))
                .collect()
        })
    }

    fn sent_back_to_read(&self, calls: &[(String, String, Value)], new_rules: &[String]) -> Vec<Block> {
        calls
            .iter()
            .map(|(id, name, _)| {
                let content = format!(
                    "New project instructions apply. Read these rules and issue this tool call again:\n{}",
                    new_rules.join("\n\n")
                );
                (self.emit)(EngineEvent::ToolStarted(ToolCall::new(id, name, "Loading project rules")));
                (self.emit)(EngineEvent::ToolFinished { id: id.clone() });
                (self.emit)(EngineEvent::ToolResult { id: id.clone(), output: content.clone(), failed: true });
                Block::ToolResult { id: id.clone(), content, is_error: true }
            })
            .collect()
    }

    fn side_by_side(&self, chunk: &[(String, String, Value)], run: &ThisRun) -> Vec<Block> {
        std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|(id, name, input)| scope.spawn(move || self.call_from(id, name, input, run, &[], None)))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)))
                .collect()
        })
    }

    pub(in crate::agent) fn concurrent(&self, run: &ThisRun, name: &str, input: &Value) -> bool {
        let named = ToolName::of(name);
        let serial = named.is_a_task_tool()
            || matches!(
                named,
                ToolName::AskUserQuestion
                    | ToolName::ExitPlanMode
                    | ToolName::Agent
                    | ToolName::SendMessage
                    | ToolName::Skill
            );
        let Some(tool) = self.tool(run, name) else { return false };
        if self.invalid_input(name, tool, input).is_some() {
            return false;
        }
        let effects = tool.effects(input, self.workspace);
        domain::permission::change_nothing(&effects)
            && !serial
            && !tool.serial()
            && self.workspace.stays_inside(&effects)
    }
}

fn batches(lanes: &[Lane]) -> Vec<std::ops::Range<usize>> {
    let mut groups: Vec<std::ops::Range<usize>> = Vec::new();
    for (at, &lane) in lanes.iter().enumerate() {
        match groups.last_mut() {
            Some(last) if lane != Lane::Alone && lanes[last.start] == lane => last.end = at + 1,
            _ => groups.push(at..at + 1),
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::Lane::{Agents, Alone, SideBySide};

    #[test]
    fn parallel_calls_group_between_the_serial_ones_in_order() {
        let groups = super::batches(&[SideBySide, SideBySide, Alone, Alone, SideBySide, SideBySide, SideBySide]);
        assert_eq!(groups, [0..2, 2..3, 3..4, 4..7]);
        assert!(super::batches(&[]).is_empty());
    }

    #[test]
    fn agent_calls_next_to_each_other_form_their_own_group() {
        let groups = super::batches(&[Agents, Agents, SideBySide, Agents, Alone, Agents, Agents, Agents]);
        assert_eq!(groups, [0..2, 2..3, 3..4, 4..5, 5..8]);
    }
}
