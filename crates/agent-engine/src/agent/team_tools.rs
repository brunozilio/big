use std::path::PathBuf;

use domain::team::{LEAD, Member, coordination, sent_message};
use serde_json::Value;

use super::Agent;
use crate::team::launch::{Launching, launch};
use crate::team::{Team, lead_inbox};

impl Agent<'_> {
    fn home(&self) -> Option<PathBuf> {
        self.workspace.home.clone()
    }

    fn sender(&self) -> Option<(Team, String)> {
        crate::team::current(
            self.workspace.home.as_deref(),
            &self.workspace.session_id,
            self.workspace.teammate.as_ref(),
        )
        .map(|me| (me.team, me.name))
    }

    pub(super) fn start_teammate(&self, input: &Value) -> Result<(String, String), String> {
        let wanted = input["name"].as_str().map(str::trim).unwrap_or_default();
        let home = self.home().ok_or("teammates need a home folder to keep their team in")?;
        if self.depth > 0 || self.workspace.teammate.is_some() {
            return Err(
                "Only the lead starts teammates; ask the lead (SendMessage to team-lead) if you need help.".into()
            );
        }
        let prompt = input["prompt"].as_str().map(str::trim).filter(|prompt| !prompt.is_empty());
        let prompt = prompt.ok_or("Missing required parameter `prompt`: the teammate's task.")?;
        let kind = input["subagent_type"].as_str().unwrap_or("general-purpose");
        let found = self.agents.iter().find(|agent| agent.name == kind);
        let session = &self.workspace.session_id;
        let team = Team::of_session(&home, session);
        team.open(session)?;
        let model = input["model"]
            .as_str()
            .map(|named| self.workspace.models.resolved(named, &self.request.model).to_owned())
            .or_else(|| found.and_then(|agent| agent.config.model_in(&self.request.model, &self.workspace.models).ok()))
            .unwrap_or_else(|| self.request.model.clone());
        let cwd = self.workspace.cwd.clone();
        let member = Member {
            agent_type: kind.to_owned(),
            model: Some(model.clone()),
            cwd: cwd.display().to_string(),
            ..Member::default()
        };
        let name = team.join(wanted, member)?;
        let mut instructions = coordination(&name, &team.name, &team.config_path().display().to_string());
        if let Some(agent) = found.filter(|agent| !agent.instructions.trim().is_empty()) {
            instructions.push_str(&format!("\n\n# Your instructions ({})\n{}", agent.name, agent.instructions));
        }
        let launching = Launching {
            team: &team,
            name: &name,
            cwd: &cwd,
            prompt,
            instructions: &instructions,
            model: &model,
            mode: self.control.mode(),
            lead: Some(self.workspace.agent_name.as_str()).filter(|name| *name != "main"),
        };
        if let Err(error) = launch(&launching) {
            team.left(&name);
            return Err(error);
        }
        lead_inbox::watch(&team, self.workspace.credentials.client);
        Ok((name, team.name.clone()))
    }

    pub(super) fn team_message(&self, input: &Value) -> Option<Result<String, String>> {
        let to = input["to"].as_str().map(str::trim)?;
        let (team, me) = self.sender()?;
        let to = if to == "main" { LEAD } else { to };
        if !team.knows(to) {
            return None;
        }
        let summary = input["summary"].as_str().map(str::trim).filter(|summary| !summary.is_empty());
        let sent = sent_message(&me, &input["message"], || uuid::Uuid::new_v4().to_string())
            .and_then(|text| team.send(to, &me, &text, summary).map_err(String::from));
        Some(sent.map(|()| to.to_owned()))
    }
}
