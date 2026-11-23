use std::path::Path;
use std::sync::Arc;

use domain::redact::Known;
use domain::web_reading::WebReading;

use crate::tools::script_tool::ScriptGrants;

#[derive(Debug, Default)]
pub struct Policy {
    pub pre_approved: Vec<String>,
    pub scripts: ScriptGrants,
    pub redact: Option<Arc<Known>>,
    pub escalation: bool,
    pub auto_review: bool,
    pub follow_through: bool,
    pub affected_tests: bool,
    pub session_claims: bool,
    pub load_on_call: bool,
    pub code_mode: domain::code_mode::Mode,
    pub teammate_mode: domain::team::TeammateMode,
    pub code_mode_detach: Option<std::time::Duration>,
    pub web: WebReading,
    pub output: domain::output::Shaping,
    pub bash_output: domain::output::OutputLimit,
}

impl Policy {
    pub fn fork(&self) -> Self {
        Self {
            pre_approved: Vec::new(),
            scripts: self.scripts,
            redact: self.redact.clone(),
            escalation: self.escalation,
            auto_review: self.auto_review,
            follow_through: self.follow_through,
            affected_tests: self.affected_tests,
            session_claims: self.session_claims,
            load_on_call: self.load_on_call,
            code_mode: self.code_mode,
            teammate_mode: self.teammate_mode,
            code_mode_detach: self.code_mode_detach,
            web: self.web,
            output: self.output,
            bash_output: self.bash_output,
        }
    }

    pub fn redacted(&self, text: &str) -> String {
        match &self.redact {
            Some(known) => domain::redact::redact(text, known),
            None => text.to_owned(),
        }
    }
}

pub fn env_secrets(project: &Path) -> Known {
    let files = std::fs::read_dir(project).into_iter().flatten().flatten().filter(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        (name == ".env" || name.starts_with(".env.")) && !name.ends_with(".example") && !name.ends_with(".sample")
    });
    let texts: Vec<String> = files.filter_map(|entry| std::fs::read_to_string(entry.path()).ok()).collect();
    Known::from_env_files(texts.iter().map(String::as_str))
}
