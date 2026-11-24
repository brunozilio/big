use domain::EngineEvent;

use super::open::Opened;
use crate::agent::{Agent, Ending};
use crate::engine::job::Job;
use crate::tools::subagent::FoundAgent;
use crate::transcript::Usage;

impl Job {
    pub(in crate::engine) fn drive(&self, opened: &mut Opened, agents: &[FoundAgent]) -> Result<Usage, Ending> {
        let emit = |event: EngineEvent| {
            self.gate.emit(event);
        };
        let agent = Agent {
            api: &opened.api,
            tools: &opened.toolbox.tools,
            workspace: &opened.workspace,
            control: &self.control,
            emit: &emit,
            request: opened.template.clone(),
            agents,
            depth: 0,
            turn_limit: opened.turn_limit,
        };
        agent.run_keeping(&mut opened.history, self.kept_at_every_transition())
    }
}
