use std::path::Path;
use std::sync::Arc;

use domain::redact::{FILE, Known, Redactor, Severity};
use domain::web_reading::WebReading;

use crate::tools::script_tool::ScriptGrants;
use crate::transcript::{Block, Message, Role};

#[derive(Debug, Default)]
pub struct Policy {
    pub scripts: ScriptGrants,
    pub redact: Option<Arc<Known>>,
    pub redactor: Arc<Redactor>,
    pub escalation: bool,
    pub auto_review: bool,
    pub follow_through: bool,
    pub affected_tests: bool,
    pub session_claims: bool,
    pub load_on_call: bool,
    pub code_mode: domain::code_mode::Mode,
    pub code_mode_detach: Option<std::time::Duration>,
    pub web: WebReading,
    pub output: domain::output::Shaping,
    pub bash_output: domain::output::OutputLimit,
}

impl Policy {
    pub fn fork(&self) -> Self {
        Self {
            scripts: self.scripts,
            redact: self.redact.clone(),
            redactor: Arc::clone(&self.redactor),
            escalation: self.escalation,
            auto_review: self.auto_review,
            follow_through: self.follow_through,
            affected_tests: self.affected_tests,
            session_claims: self.session_claims,
            load_on_call: self.load_on_call,
            code_mode: self.code_mode,
            code_mode_detach: self.code_mode_detach,
            web: self.web,
            output: self.output,
            bash_output: self.bash_output,
        }
    }

    pub fn redacted(&self, text: &str) -> String {
        match &self.redact {
            Some(known) => self.redactor.redact(text, known).text,
            None => text.to_owned(),
        }
    }

    pub fn holds_a_secret(&self, text: &str) -> bool {
        match &self.redact {
            Some(known) => self.redactor.redact(text, known).sensitive(),
            None => self.redactor.redact(text, &Known::default()).sensitive(),
        }
    }

    pub fn redact_prompt(&self, messages: &mut [Message]) {
        let Some(known) = &self.redact else { return };
        for message in messages.iter_mut().filter(|message| message.role == Role::User) {
            for block in &mut message.blocks {
                if let Block::Text(text) | Block::Reminder(text) = block {
                    *text = self.redactor.redact(text, known).text;
                }
            }
        }
    }
}

fn told(problems: Vec<domain::redact::Problem>) {
    for problem in problems {
        match problem.severity {
            Severity::Error => tracing::error!(%problem, "redaction rules"),
            Severity::Warning => tracing::warn!(%problem, "redaction rules"),
        }
    }
}

pub fn redactor() -> Arc<Redactor> {
    let shipped = content::builtin_text(FILE);
    let mine = content::user_text(content::folder().as_deref(), FILE);
    let (redactor, problems) = domain::redact::assemble(shipped.as_deref(), mine.as_deref(), &[]);
    told(problems);
    Arc::new(redactor)
}

pub fn personal(base: &Redactor) -> Arc<Redactor> {
    let file = domain::redact::PERSONAL_FILE;
    let shipped = content::builtin_text(file);
    let mine = content::user_text(content::folder().as_deref(), file);
    let (redactor, problems) = domain::redact::widened(base, shipped.as_deref(), mine.as_deref());
    told(problems);
    Arc::new(redactor)
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
