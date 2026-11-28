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
            Self::Tool(text) => clean(text).filter(|tool| tool_named(tool)).map(Self::Tool),
            Self::Site(text) => clean(site_of(&text)).map(Self::Site),
        }
    }
}

fn tool_named(tool: &str) -> bool {
    let word = |part: &str| {
        !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    };
    match tool.strip_prefix("mcp__").map(|rest| rest.split_once("__")) {
        Some(Some((server, name))) => word(server) && (name == "*" || word(name.strip_suffix('*').unwrap_or(name))),
        Some(None) => false,
        None => tool.starts_with(|c: char| c.is_ascii_uppercase()) && tool.chars().all(|c| c.is_ascii_alphanumeric()),
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Behavior {
    #[default]
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rule {
    #[serde(flatten)]
    pub grant: Grant,
    #[serde(default)]
    pub behavior: Behavior,
}

impl Rule {
    pub fn new(grant: Grant, behavior: Behavior) -> Self {
        Self { grant, behavior }
    }

    pub fn loose(&self) -> bool {
        self.behavior != Behavior::Deny && self.grant.clone().tidy().is_some_and(|grant| grant.everything())
    }
}

impl Grant {
    fn everything(&self) -> bool {
        match self {
            Self::Command(rule) => rule == "*",
            Self::Site(rule) => rule == "*" || rule.strip_prefix("*.").is_some_and(|parent| !parent.contains('.')),
            Self::Tool(_) => false,
        }
    }
}

impl From<Grant> for Rule {
    fn from(grant: Grant) -> Self {
        Self { grant, behavior: Behavior::Allow }
    }
}

fn command_matches(rule: &str, words: &[String]) -> bool {
    if rule == "*" {
        return true;
    }
    let wanted: Vec<&str> = rule.split_whitespace().collect();
    let wanted = if wanted.last() == Some(&"*") { &wanted[..wanted.len() - 1] } else { &wanted[..] };
    words.len() >= wanted.len() && wanted.iter().zip(words).all(|(want, word)| *want == word)
}

fn tool_matches(rule: &str, tool: &str) -> bool {
    match rule.strip_suffix('*') {
        Some(prefix) => tool.starts_with(prefix),
        None => rule == tool,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Allowed {
    rules: Vec<Rule>,
}

impl Allowed {
    pub fn of(rules: impl IntoIterator<Item = impl Into<Rule>>) -> Self {
        let mut allowed = Self::default();
        for rule in rules {
            allowed.add(rule);
        }
        allowed
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn has(&self, grant: &Grant) -> bool {
        self.rules.iter().any(|rule| rule.grant == *grant)
    }

    fn put(&mut self, rule: Rule, first: bool) -> bool {
        if rule.loose() {
            return false;
        }
        let Some(grant) = rule.grant.tidy() else { return false };
        if let Some(known) = self.rules.iter_mut().find(|known| known.grant == grant) {
            let changed = known.behavior != rule.behavior;
            known.behavior = rule.behavior;
            return changed;
        }
        let rule = Rule { grant, behavior: rule.behavior };
        if first {
            self.rules.insert(0, rule)
        } else {
            self.rules.push(rule)
        }
        true
    }

    pub fn add(&mut self, rule: impl Into<Rule>) -> bool {
        self.put(rule.into(), false)
    }

    pub fn add_first(&mut self, rule: impl Into<Rule>) -> bool {
        self.put(rule.into(), true)
    }

    pub fn remove(&mut self, grant: &Grant) -> bool {
        let before = self.rules.len();
        let tidy = grant.clone().tidy();
        self.rules.retain(|kept| Some(&kept.grant) != tidy.as_ref());
        before != self.rules.len()
    }

    fn grants(&self, behavior: Behavior) -> impl Iterator<Item = &Grant> {
        self.rules.iter().filter(move |rule| rule.behavior == behavior).map(|rule| &rule.grant)
    }

    fn site_is(&self, host: &str, behavior: Behavior) -> bool {
        let host = crate::site::site(host.trim()).unwrap_or_default();
        self.grants(behavior).any(|grant| matches!(grant, Grant::Site(site) if site_matches(site, &host)))
    }

    pub fn site_allowed(&self, host: &str) -> bool {
        self.site_is(host, Behavior::Allow)
    }

    pub fn sites(&self) -> Vec<String> {
        self.grants(Behavior::Allow)
            .filter_map(|grant| match grant {
                Grant::Site(site) => Some(site.clone()),
                _ => None,
            })
            .collect()
    }

    fn tool_is(&self, tool: &str, behavior: Behavior) -> bool {
        self.grants(behavior).any(|grant| matches!(grant, Grant::Tool(rule) if tool_matches(rule, tool)))
    }

    fn command_allowed(&self, command: &ShellCommand) -> bool {
        if command.opaque() {
            return self.grants(Behavior::Allow).any(|grant| *grant == Grant::Command("*".to_owned()));
        }
        let commands = command.commands();
        !commands.is_empty() && commands.iter().all(|simple| self.simple_allowed(simple))
    }

    fn simple_allowed(&self, simple: &str) -> bool {
        let parsed = SimpleCommand::of(simple);
        let writes = parsed.as_ref().is_none_or(|parsed| parsed.redirections.iter().any(writes_a_file));
        let words = parsed.map(|parsed| parsed.words).unwrap_or_default();
        self.grants(Behavior::Allow).any(|grant| match grant {
            Grant::Command(rule) if rule == "*" => true,
            Grant::Command(_) if writes => false,
            Grant::Command(rule) => command_matches(rule, &words),
            _ => false,
        })
    }

    fn command_is(&self, command: &ShellCommand, behavior: Behavior) -> bool {
        let rules: Vec<&str> = self
            .grants(behavior)
            .filter_map(|grant| match grant {
                Grant::Command(rule) => Some(rule.as_str()),
                _ => None,
            })
            .collect();
        if command.opaque() {
            return rules.contains(&"*");
        }
        command.commands().into_iter().any(|simple| {
            let words = SimpleCommand::of(simple).map(|parsed| parsed.words).unwrap_or_default();
            rules.iter().any(|rule| command_matches(rule, &words))
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

    fn touches(&self, call: &Call<'_>, behavior: Behavior) -> bool {
        self.tool_is(call.tool, behavior)
            || call.effects.iter().any(|effect| match effect {
                Effect::Exec(exec) => self.command_is(&exec.command, behavior),
                Effect::Net { host: Some(host) } | Effect::Fetch { host: Some(host) } => self.site_is(host, behavior),
                _ => false,
            })
    }

    pub fn covers(&self, call: &Call<'_>) -> bool {
        self.tool_is(call.tool, Behavior::Allow) || call.effects.iter().all(|effect| self.effect_allowed(effect))
    }

    pub fn verdict(&self, call: &Call<'_>) -> Option<Behavior> {
        if self.touches(call, Behavior::Deny) {
            return Some(Behavior::Deny);
        }
        if self.touches(call, Behavior::Ask) {
            return Some(Behavior::Ask);
        }
        self.covers(call).then_some(Behavior::Allow)
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
    "ls",
    "pwd",
    "cat",
    "head",
    "tail",
    "wc",
    "grep",
    "rg",
    "tree",
    "which",
    "file",
    "stat",
    "du",
    "df",
    "sort",
    "uniq",
    "diff",
    "echo",
    "date",
    "uname",
    "whoami",
    "git status",
    "git diff",
    "git log",
    "git show",
    "git rev-parse",
    "git blame",
    "git ls-files",
    "cargo check",
    "cargo build",
    "cargo test",
    "cargo nextest",
    "cargo clippy",
    "cargo fmt",
    "cargo tree",
    "cargo metadata",
    "bun test",
    "npm test",
    "node --version",
    "python3 --version",
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
