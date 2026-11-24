use std::sync::{Arc, LazyLock, PoisonError, RwLock};

use serde::{Deserialize, Serialize};

use super::{Call, Effect};
use crate::shell::{ShellCommand, SimpleCommand};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Grant {
    Command(String),
    Site(String),
    Tool(String),
}

impl Grant {
    pub fn tidy(self) -> Option<Self> {
        let clean =
            |text: String| Some(text.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|t| !t.is_empty());
        match self {
            Self::Command(text) => clean(text).map(Self::Command),
            Self::Tool(text) => clean(text).map(Self::Tool),
            Self::Site(text) => clean(site_of(&text)).map(Self::Site),
        }
    }
}

fn site_of(text: &str) -> String {
    let text = text.trim();
    if text == "*" {
        return text.to_owned();
    }
    match text.strip_prefix("*.") {
        Some(rest) => crate::site::site(rest).map(|site| format!("*.{site}")).unwrap_or_default(),
        None => crate::site::site(text).unwrap_or_default(),
    }
}

fn site_matches(rule: &str, host: &str) -> bool {
    match rule.strip_prefix("*.") {
        Some(parent) => host.ends_with(&format!(".{parent}")),
        None => rule == "*" || rule == host,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Allowed {
    grants: Vec<Grant>,
}

impl Allowed {
    pub fn of(grants: impl IntoIterator<Item = Grant>) -> Self {
        let mut allowed = Self::default();
        for grant in grants {
            allowed.add(grant);
        }
        allowed
    }

    pub fn grants(&self) -> &[Grant] {
        &self.grants
    }

    pub fn add(&mut self, grant: Grant) -> bool {
        match grant.tidy() {
            Some(grant) if !self.grants.contains(&grant) => {
                self.grants.push(grant);
                true
            }
            _ => false,
        }
    }

    pub fn remove(&mut self, grant: &Grant) -> bool {
        let before = self.grants.len();
        let tidy = grant.clone().tidy();
        self.grants.retain(|kept| Some(kept) != tidy.as_ref());
        before != self.grants.len()
    }

    pub fn site_allowed(&self, host: &str) -> bool {
        let host = crate::site::site(host.trim()).unwrap_or_default();
        self.grants.iter().any(|grant| match grant {
            Grant::Site(site) => site_matches(site, &host),
            _ => false,
        })
    }

    pub fn sites(&self) -> Vec<String> {
        self.grants
            .iter()
            .filter_map(|grant| match grant {
                Grant::Site(site) => Some(site.clone()),
                _ => None,
            })
            .collect()
    }

    fn tool_allowed(&self, tool: &str) -> bool {
        self.grants.iter().any(|grant| match grant {
            Grant::Tool(rule) => match rule.strip_suffix('*') {
                Some(prefix) => tool.starts_with(prefix),
                None => rule == tool,
            },
            _ => false,
        })
    }

    fn command_allowed(&self, command: &ShellCommand) -> bool {
        if command.opaque() {
            return self.grants.contains(&Grant::Command("*".to_owned()));
        }
        let commands = command.commands();
        !commands.is_empty() && commands.iter().all(|simple| self.simple_allowed(simple))
    }

    fn simple_allowed(&self, simple: &str) -> bool {
        let parsed = SimpleCommand::of(simple);
        let writes = parsed.as_ref().is_none_or(|parsed| parsed.redirections.iter().any(writes_a_file));
        let words = parsed.map(|parsed| parsed.words).unwrap_or_default();
        self.grants.iter().any(|grant| match grant {
            Grant::Command(rule) if rule == "*" => true,
            Grant::Command(_) if writes => false,
            Grant::Command(rule) => {
                let wanted: Vec<&str> = rule.split_whitespace().collect();
                let last_is_star = wanted.last() == Some(&"*");
                let wanted = if last_is_star { &wanted[..wanted.len() - 1] } else { &wanted[..] };
                words.len() >= wanted.len() && wanted.iter().zip(&words).all(|(want, word)| *want == word)
            }
            _ => false,
        })
    }

    fn effect_allowed(&self, effect: &Effect) -> bool {
        let browser = crate::mcp::browser_server();
        match effect {
            Effect::Exec(exec) => self.command_allowed(&exec.command),
            Effect::Net { host: Some(host) } | Effect::Fetch { host: Some(host) } => self.site_allowed(host),
            Effect::Remote { server } if *server == browser => true,
            other => !other.acts(),
        }
    }

    pub fn covers(&self, call: &Call<'_>) -> bool {
        self.tool_allowed(call.tool) || call.effects.iter().all(|effect| self.effect_allowed(effect))
    }

    pub fn learned(call: &Call<'_>) -> Vec<Grant> {
        let browser = crate::mcp::browser_server();
        let mut learned = Vec::new();
        let mut whole_tool = false;
        for effect in call.effects.iter().filter(|effect| effect.acts()) {
            match effect {
                Effect::Exec(exec) if !exec.command.opaque() => {
                    learned.extend(exec.command.commands().into_iter().map(|simple| Grant::Command(prefix(simple))));
                }
                Effect::Net { host: Some(host) } | Effect::Fetch { host: Some(host) } => {
                    learned.push(Grant::Site(host.clone()));
                }
                Effect::Remote { server } if *server == browser => {}
                Effect::Fetch { host: None } if call.effects.iter().any(is_browser) => {}
                _ => whole_tool = true,
            }
        }
        if whole_tool {
            return vec![Grant::Tool(call.tool.to_owned())];
        }
        learned
    }
}

fn writes_a_file(redirection: &crate::shell::Redirection) -> bool {
    !redirection.reads_a_file() && !matches!(redirection.target.as_str(), "/dev/null" | "&1" | "&2" | "1" | "2")
}

const DEFAULT_COMMANDS: &[&str] = &[
    "ls", "pwd", "cat", "head", "tail", "wc", "grep", "rg", "tree", "which", "file", "stat", "du", "df", "sort",
    "uniq", "diff", "echo", "date", "uname", "whoami", "git status", "git diff", "git log", "git show",
    "git rev-parse", "git blame", "git ls-files", "cargo check", "cargo build",
    "cargo test", "cargo nextest", "cargo clippy", "cargo fmt", "cargo tree", "cargo metadata", "bun test",
    "npm test", "node --version", "python3 --version",
];

pub fn defaults() -> Vec<Grant> {
    DEFAULT_COMMANDS.iter().map(|command| Grant::Command((*command).to_owned())).collect()
}

fn is_browser(effect: &Effect) -> bool {
    matches!(effect, Effect::Remote { server } if *server == crate::mcp::browser_server())
}

fn prefix(simple: &str) -> String {
    let words = SimpleCommand::of(simple).map(|parsed| parsed.words).unwrap_or_default();
    let mut kept: Vec<&str> = Vec::new();
    for word in &words {
        let plain = word.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'));
        if kept.len() == 2 || word.starts_with('-') || !plain {
            break;
        }
        kept.push(word);
    }
    if kept.is_empty() { simple.trim().to_owned() } else { kept.join(" ") }
}

type Saver = Arc<dyn Fn(&Allowed) + Send + Sync>;

static GLOBAL: LazyLock<RwLock<Option<Allowed>>> = LazyLock::new(RwLock::default);
static SAVER: LazyLock<RwLock<Option<Saver>>> = LazyLock::new(RwLock::default);

pub fn install(allowed: Allowed, saver: Saver) {
    *SAVER.write().unwrap_or_else(PoisonError::into_inner) = Some(saver);
    *GLOBAL.write().unwrap_or_else(PoisonError::into_inner) = Some(allowed);
}

pub fn global() -> Option<Allowed> {
    GLOBAL.read().unwrap_or_else(PoisonError::into_inner).clone()
}

pub fn change(apply: impl FnOnce(&mut Allowed)) -> Option<Allowed> {
    let changed = {
        let mut global = GLOBAL.write().unwrap_or_else(PoisonError::into_inner);
        let allowed = global.as_mut()?;
        apply(allowed);
        allowed.clone()
    };
    let saver = SAVER.read().unwrap_or_else(PoisonError::into_inner).clone();
    if let Some(saver) = saver {
        saver(&changed);
    }
    Some(changed)
}
