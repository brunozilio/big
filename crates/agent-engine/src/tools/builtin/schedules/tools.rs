use super::*;

#[derive(Debug)]
pub struct ScheduleWakeup;

const WAKE: (i64, i64) = (60, 3600);

impl Tool for ScheduleWakeup {
    fn spec(&self) -> ToolSpec {
        crate::tools::declared("ScheduleWakeup", &serde_json::Value::Null)
    }

    fn effects(&self, _input: &Value, _workspace: &Workspace) -> Vec<crate::tools::Effect> {
        vec![crate::tools::Effect::Spawn(crate::tools::SpawnKind::Schedule)]
    }

    fn summary(&self, input: &Value) -> String {
        field(input, "reason").unwrap_or_default().to_owned()
    }

    fn run(&self, input: &Value, workspace: &Workspace) -> Outcome {
        let prompt = match required(input, "prompt") {
            Ok(prompt) => prompt,
            Err(error) => return error,
        };
        let Some(delay) = input["delaySeconds"].as_f64() else {
            return Outcome::error("Missing required parameter `delaySeconds`.");
        };
        let delay = (delay as i64).clamp(WAKE.0, WAKE.1);
        match add(workspace.credentials.client, prompt, now() + delay) {
            Ok(scheduled) => Outcome::ok(format!("Wakeup in {delay}s: {}", shown(&scheduled))),
            Err(error) => Outcome::error(error),
        }
    }
}
