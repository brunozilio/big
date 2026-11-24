use std::path::Path;
use std::sync::Arc;

use brand::features as flags;
use domain::features::on;
use domain::model_roles::ModelRoles;
use domain::{PromptRequest, SessionId};

use super::equip::{Equipped, MainAgent, Toolbox};
use crate::brief::Brief;
use crate::engine::job::Job;
use crate::engine::roles;
use crate::engine::stop::Stop;
use crate::prompt::settings::Settings;
use crate::tools::{self, Policy, Workspace};

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
        toolbox.tools.retain(|tool| tool.offered(&workspace));
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
        web: domain::web_reading::WebReading::configured(),
        output: domain::output::Shaping::configured(),
        bash_output: settings.bash_output,
        ..Policy::default()
    }
}

pub(in crate::engine) fn budget(request: &PromptRequest) -> Option<domain::budget::Budget> {
    request.policy.budget.filter(|_| on(flags::BUDGET))
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
