use std::sync::Arc;

use crate::brief::Brief;
use crate::engine::job::Job;
use crate::engine::stop::Stop;
use crate::mcp::McpServer;
use crate::tools::subagent::{self, AgentTool, FoundAgent};
use domain::MainAgentSpec;
use crate::tools::subagent_config::AgentConfig;
use crate::tools::{self, Tool, Workspace};

pub(in crate::engine) struct Toolbox {
    pub(in crate::engine) tools: Vec<Box<dyn Tool>>,
    pub(in crate::engine) servers: Vec<Arc<McpServer>>,
}

pub(in crate::engine) struct MainAgent {
    pub(in crate::engine) found: Option<FoundAgent>,
    pub(in crate::engine) config: AgentConfig,
}

impl MainAgent {
    pub(in crate::engine) fn chosen(brief: &Brief, asked: Option<&MainAgentSpec>) -> Result<Self, String> {
        let found = match asked {
            Some(spec) => Some(Self::asked(&brief.agents, spec)?),
            None => match &brief.settings.agent {
                Some(name) => AgentTool { agents: brief.agents.clone() }
                    .chosen(&serde_json::json!({"subagent_type":name}))?
                    .cloned(),
                None => None,
            },
        };
        let config = found.as_ref().map(|agent| agent.config.clone()).unwrap_or_default();
        Ok(Self { found, config })
    }

    fn asked(agents: &[FoundAgent], spec: &MainAgentSpec) -> Result<FoundAgent, String> {
        let mut found = match &spec.definition {
            Some(text) => subagent::parsed(&spec.name, &text.replace("{{NAME}}", brand::NAME), None, true),
            None => AgentTool { agents: agents.to_vec() }
                .chosen(&serde_json::json!({"subagent_type": spec.name}))?
                .cloned()
                .ok_or_else(|| format!("There is no agent named {}.", spec.name))?,
        };
        if let Some(error) = &found.config.error {
            return Err(format!("Agent {}: {error}", found.name));
        }
        found.config.permission = None;
        found.config.model = None;
        found.config.effort = None;
        Ok(found)
    }

    pub(in crate::engine) fn name(&self) -> &str {
        self.found.as_ref().map_or("main", |agent| agent.name.as_str())
    }
}

pub(in crate::engine) struct Equipped {
    pub(in crate::engine) workspace: Workspace,
    pub(in crate::engine) toolbox: Toolbox,
    pub(in crate::engine) main: MainAgent,
}

impl Job {
    pub(in crate::engine) fn equip(&mut self, brief: &Brief, mut workspace: Workspace) -> Result<Equipped, Stop> {
        workspace.rules.clone_from(&brief.rules);
        let mut tools = tools::standard();
        self.offer_language_servers(brief, &mut workspace, &mut tools);
        if !brief.skills.is_empty() {
            tools.push(Box::new(tools::skill::SkillTool { skills: brief.skills.clone() }));
        }
        tools.push(Box::new(AgentTool { agents: brief.agents.clone() }));
        tools.push(Box::new(tools::send_message::SendMessageTool));
        tools.push(Box::new(tools::repl::Repl));
        tools.extend(tools::task::tools());
        tools.extend(tools::hosted::tools(self.request.host.credentials.client));
        let main = MainAgent::chosen(brief, self.request.session.agent.as_ref()).map_err(Stop::failed)?;
        self.control = self.control.reconfigured(main.config.permission, main.config.dont_ask);
        workspace.stop = self.control.shared_stop();
        workspace.observing = main.config.observing();
        let stop = self.control.stop_flag();
        let servers = self.live.connect(&self.request, &brief.mcp(), &self.mcp, stop);
        tools.extend(crate::mcp::tools_of(&servers, stop));
        tools::script_tool::join(&mut tools, self.live.scripts(brief, &self.request.session.cwd));
        if let Some(expanded) = crate::mcp::extras::expand_prompt(&servers, &self.request.prompt.text, stop) {
            self.request.prompt.text = expanded.map_err(Stop::failed)?;
        }
        Ok(Equipped { workspace, toolbox: Toolbox { tools, servers }, main })
    }

    fn offer_language_servers(&self, brief: &Brief, workspace: &mut Workspace, tools: &mut Vec<Box<dyn Tool>>) {
        let specs = domain::lsp::servers(domain::lsp::defaults(), &brief.settings.lsp_servers);
        let lsp = crate::lsp::Lsp { servers: Arc::clone(&self.lsp), specs: Arc::new(specs) };
        if !lsp.available().is_empty() {
            workspace.services.lsp = Some(lsp);
            tools.push(Box::new(tools::Lsp));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVELOPER: &str = "---\nname: dev\ntools: [Read, Agent, \"mcp__*\"]\ndisallowedTools: [Write]\n\
permissionMode: bypass\nmodel: opus\nskills: [verify]\n---\nVocê trabalha no {{NAME}}.";

    #[test]
    fn a_requested_agent_brings_its_tools_and_skills_but_the_requester_keeps_the_mode_and_the_model() {
        let spec = MainAgentSpec { name: "dev".into(), definition: Some(DEVELOPER.into()) };
        let found = MainAgent::asked(&[], &spec).expect("chosen");
        assert_eq!(found.name, "dev");
        assert!(["Read", "Agent", "mcp__docs__search"].iter().all(|tool| found.config.allows(tool)));
        assert!(!found.config.allows("Write") && !found.config.allows("Bash"));
        assert_eq!(found.config.skills, ["verify"]);
        assert_eq!((found.config.permission, found.config.model.as_deref()), (None, None));
        assert_eq!(found.instructions, format!("Você trabalha no {}.", brand::NAME));
    }

    #[test]
    fn a_requested_agent_without_a_definition_is_looked_up_by_name() {
        let known = subagent::parsed("dev", DEVELOPER, None, true);
        let spec = MainAgentSpec { name: "dev".into(), definition: None };
        assert_eq!(MainAgent::asked(std::slice::from_ref(&known), &spec).expect("found").name, "dev");
        let missing = MainAgentSpec { name: "ninguem".into(), definition: None };
        assert!(MainAgent::asked(&[known], &missing).is_err());
    }
}
