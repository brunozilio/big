use std::path::Path;
use std::sync::Arc;

use brand::features as flags;
use domain::features::on;
use domain::model_roles::{ModelRole, ModelRoles};
use domain::{PromptRequest, SessionId};

use super::equip::{Equipped, MainAgent, Toolbox};
use crate::brief::Brief;
use crate::engine::job::Job;
use crate::engine::roles;
use crate::engine::stop::Stop;
use crate::prompt::settings::Settings;
use crate::tools::subagent_config::AgentConfig;
use crate::tools::{self, Policy, Verification, Workspace};

pub(in crate::engine) struct Governed {
    pub(in crate::engine) workspace: Workspace,
    pub(in crate::engine) toolbox: Toolbox,
    pub(in crate::engine) main: MainAgent,
    pub(in crate::engine) roles: ModelRoles,
}

impl Job {
    pub(in crate::engine) fn apply_policy(
        &mut self,
        equipped: Equipped,
        brief: &Brief,
        session: &SessionId,
    ) -> Result<Governed, Stop> {
        let settings = &brief.settings;
        let Equipped { mut workspace, mut toolbox, main } = equipped;
        workspace.policy = flagged(settings, &workspace.cwd);
        workspace.models.escalation.clone_from(&settings.escalation);
        self.request.policy.budget = budget(&self.request);
        workspace.services.files = Some(Arc::default());
        let roles = roles::merged(&self.request.session.roles, settings);
        if let Some(advisor) = roles.get(ModelRole::Advisor) {
            let model = role_model(&self.request, advisor).map_err(|error| format!("advisorModel: {error}"));
            workspace.models.advisor = Some(model.map_err(Stop::failed)?);
        }
        toolbox.tools.retain(|tool| tool.offered(&workspace));
        workspace.verification = verification(&self.request, settings, &roles).map_err(Stop::failed)?;
        workspace.agent_name = main.name().into();
        workspace.session_id.clone_from(&session.0);
        workspace.anchored_memories = anchored(&self.request);
        workspace.blast_radius = on(flags::BLAST_RADIUS);
        workspace.hooks = settings.hooks.clone();
        Ok(Governed { workspace, toolbox, main, roles })
    }
}

pub(in crate::engine) fn flagged(settings: &Settings, cwd: &Path) -> Policy {
    Policy {
        scripts: settings.scripts,
        redact: on(flags::REDACT_SECRETS).then(|| Arc::new(tools::env_secrets(cwd))),
        escalation: on(flags::ESCALATION),
        auto_review: on(flags::AUTO_REVIEW),
        follow_through: on(flags::FOLLOW_THROUGH),
        affected_tests: on(flags::AFFECTED_TESTS),
        session_claims: on(flags::SESSION_CLAIMS),
        load_on_call: on(flags::LOAD_ON_CALL),
        code_mode: domain::code_mode::Mode::configured(),
        teammate_mode: settings.teammate_mode,
        web: domain::web_reading::WebReading::configured(),
        output: domain::output::Shaping::configured(),
        bash_output: settings.bash_output,
        ..Policy::default()
    }
}

pub(in crate::engine) fn budget(request: &PromptRequest) -> Option<domain::budget::Budget> {
    request.policy.budget.filter(|_| on(flags::BUDGET))
}

fn role_model(request: &PromptRequest, name: &str) -> Result<String, String> {
    let config = AgentConfig { model: Some(name.to_owned()), ..Default::default() };
    config.model(&request.session.model.id, request.session.model.provider)
}

pub(in crate::engine) fn verification(
    request: &PromptRequest,
    settings: &Settings,
    roles: &ModelRoles,
) -> Result<Option<Verification>, String> {
    let project =
        settings.project_criteria.clone().filter(|_| on(flags::PROJECT_CRITERIA)).map(domain::Criteria::project);
    let asked = request.policy.verification.clone().or(project);
    let Some(criteria) = asked.filter(|_| request.policy.permission != domain::PermissionMode::Plan) else {
        return Ok(None);
    };
    let config = AgentConfig { model: roles.get(ModelRole::Review).map(str::to_owned), ..Default::default() };
    let model = config
        .model(&request.session.model.id, request.session.model.provider)
        .map_err(|error| format!("judgeModel: {error}"))?;
    let allow = criteria.judge_commands();
    Ok(Some(Verification { criteria, model, allow }))
}

pub(in crate::engine) fn anchored(request: &PromptRequest) -> Vec<domain::Memory> {
    request
        .host
        .memory
        .as_ref()
        .filter(|brief| brief.anchors && on(flags::MEMORY_ANCHORS))
        .map(|brief| brief.anchored.clone())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
