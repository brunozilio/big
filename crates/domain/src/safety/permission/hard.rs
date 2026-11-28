use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{Call, Effect, Reason, Verdict};
use crate::shell::SimpleCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Credentials,
    InjectingEnv,
    Governance,
    PackageCache,
    SandboxEscape,
    Hidden,
}

impl Category {
    pub fn why(self) -> &'static str {
        match self {
            Self::Credentials => "it touches credentials or keys",
            Self::InjectingEnv => "it sets an environment variable that injects code into the program it runs",
            Self::Governance => concat!("it changes ", brand::name!(), "'s own configuration or content"),
            Self::PackageCache => "it writes into a package cache that other builds trust",
            Self::SandboxEscape => "it reaches past the sandbox into the machine or its host",
            Self::Hidden => "what is shown holds invisible or direction-changing characters",
        }
    }

    fn denies(self) -> bool {
        matches!(self, Self::Credentials | Self::Governance | Self::SandboxEscape)
    }

    fn on_reads(self) -> bool {
        matches!(self, Self::Credentials | Self::SandboxEscape)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Patterns {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub apps: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
}

pub type Declared = BTreeMap<Category, Patterns>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Floor {
    home: Option<PathBuf>,
    roots: Vec<PathBuf>,
    paths: Vec<(Category, PathBuf)>,
    named: Vec<(Category, String)>,
    hosts: Vec<(Category, String)>,
    apps: Vec<(Category, String)>,
    env: Vec<(Category, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub category: Category,
    pub subject: String,
    pub denies: bool,
}

impl Hit {
    fn of(category: Category, subject: impl Into<String>, denies: bool) -> Self {
        Self { category, subject: subject.into(), denies }
    }
}

impl Floor {
    pub fn at_home(home: Option<PathBuf>) -> Self {
        Self { home, ..Self::default() }
    }

    pub fn within(roots: Vec<PathBuf>) -> Self {
        Self { roots, ..Self::default() }
    }

    pub fn extend(&mut self, other: Floor) {
        self.home = self.home.take().or(other.home);
        self.roots.extend(other.roots);
        self.paths.extend(other.paths);
        self.named.extend(other.named);
        self.hosts.extend(other.hosts);
        self.apps.extend(other.apps);
        self.env.extend(other.env);
    }

    pub fn govern(&mut self, places: impl IntoIterator<Item = PathBuf>, named: impl IntoIterator<Item = String>) {
        self.paths.extend(places.into_iter().map(|place| (Category::Governance, place)));
        self.named.extend(named.into_iter().map(|name| (Category::Governance, name)));
    }

    pub fn add(&mut self, declared: &Declared, settle: &dyn Fn(&Path) -> PathBuf) {
        for (category, patterns) in declared {
            for path in patterns.paths.iter().map(|path| path.trim()).filter(|path| !path.is_empty()) {
                match self.expanded(path) {
                    Some(place) => self.paths.push((*category, settle(&place))),
                    None => self.named.push((*category, path.to_owned())),
                }
            }
            let lower = |text: &String| text.trim().to_ascii_lowercase();
            self.hosts.extend(patterns.hosts.iter().map(lower).filter(|h| !h.is_empty()).map(|h| (*category, h)));
            self.apps.extend(patterns.apps.iter().map(lower).filter(|a| !a.is_empty()).map(|a| (*category, a)));
            let env = patterns.env.iter().map(|name| name.trim().to_owned()).filter(|name| !name.is_empty());
            self.env.extend(env.map(|name| (*category, name)));
        }
    }

    fn expanded(&self, written: &str) -> Option<PathBuf> {
        let written = written.trim_matches(|c| c == '"' || c == '\'');
        for prefix in ["~/", "$HOME/", "${HOME}/"] {
            if let Some(rest) = written.strip_prefix(prefix) {
                return self.home.as_ref().map(|home| home.join(rest));
            }
        }
        if written == "~" {
            return self.home.clone();
        }
        Path::new(written).is_absolute().then(|| PathBuf::from(written))
    }

    fn path_hit(&self, real: &Path, wanted: impl Fn(Category) -> bool) -> Option<Category> {
        let by_place = self.paths.iter().find(|(category, place)| wanted(*category) && real.starts_with(place));
        let inside = self.roots.iter().find_map(|root| real.strip_prefix(root).ok()).unwrap_or(real);
        let by_name = || {
            self.named.iter().find(|(category, name)| {
                wanted(*category)
                    && inside.components().any(|part| part.as_os_str().to_string_lossy().eq_ignore_ascii_case(name))
            })
        };
        by_place.map(|(category, _)| *category).or_else(|| by_name().map(|(category, _)| *category))
    }

    fn host_hit(&self, host: &str) -> Option<Category> {
        let host = crate::site::site(host.trim()).unwrap_or_default();
        self.hosts
            .iter()
            .find(|(_, rule)| match rule.strip_prefix("*.") {
                Some(parent) => host.ends_with(&format!(".{parent}")) || host == parent,
                None => *rule == host,
            })
            .map(|(category, _)| *category)
    }

    fn env_hit(&self, text: &str) -> Option<Hit> {
        assignments(text).into_iter().find_map(|name| {
            self.env
                .iter()
                .find(|(_, rule)| match rule.strip_suffix('*') {
                    Some(prefix) => name.starts_with(prefix),
                    None => *rule == name,
                })
                .map(|(category, _)| Hit::of(*category, name, false))
        })
    }

    fn exec_hit(&self, text: &str, simple: &[&str]) -> Vec<Hit> {
        let mut hits: Vec<Hit> = self.env_hit(text).into_iter().collect();
        for command in simple {
            let parsed = SimpleCommand::of(command);
            let words: Vec<String> = parsed.as_ref().map_or_else(
                || command.split_whitespace().map(str::to_owned).collect(),
                |parsed| parsed.words.iter().chain(parsed.redirections.iter().map(|r| &r.target)).cloned().collect(),
            );
            hits.extend(words.iter().filter_map(|word| {
                let place = self.expanded(word)?;
                self.path_hit(&place, |_| true).map(|category| Hit::of(category, word.clone(), category.on_reads()))
            }));
            if let Some(program) = program(&words)
                && let Some((category, _)) = self.apps.iter().find(|(_, app)| *app == program)
            {
                hits.push(Hit::of(*category, program, false));
            }
        }
        hits
    }

    fn effect_hits(&self, effect: &Effect) -> Vec<Hit> {
        let shown = |category: Category, place: &super::Place| {
            Hit::of(category, place.written.display().to_string(), category.denies())
        };
        match effect {
            Effect::ReadPath(place) | Effect::ReadTree(place) => self
                .path_hit(&place.real, Category::on_reads)
                .map(|category| shown(category, place))
                .into_iter()
                .collect(),
            Effect::WritePath(place) => {
                self.path_hit(&place.real, |_| true).map(|category| shown(category, place)).into_iter().collect()
            }
            Effect::Exec(exec) => {
                let command = &exec.command;
                let whole = [command.text()];
                let simple = if command.opaque() { whole.to_vec() } else { command.commands() };
                self.exec_hit(command.text(), &simple)
            }
            Effect::Net { host: Some(host) } | Effect::Fetch { host: Some(host) } => self
                .host_hit(host)
                .map(|category| Hit::of(category, host.clone(), category.denies()))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn hits(&self, call: &Call<'_>) -> Vec<Hit> {
        let mut hits: Vec<Hit> = call.effects.iter().flat_map(|effect| self.effect_hits(effect)).collect();
        if !hidden(call.subject).is_empty() {
            hits.push(Hit::of(Category::Hidden, revealed(call.subject), false));
        }
        hits
    }

    pub fn verdict(&self, call: &Call<'_>) -> Option<Verdict> {
        let hits = self.hits(call);
        if let Some(hit) = hits.iter().find(|hit| hit.denies) {
            return Some(Verdict::Deny(format!(
                "{} is blocked by the safety floor: {} ({}). No setting, reviewer or 'always' lifts this.",
                call.tool,
                hit.category.why(),
                hit.subject
            )));
        }
        hits.first().map(|hit| Verdict::Ask { reason: Reason::Hard(hit.category), rememberable: false })
    }

    pub fn tightened(&self, call: &Call<'_>, verdict: Verdict) -> Verdict {
        match (self.verdict(call), verdict) {
            (_, Verdict::Deny(reason)) | (Some(Verdict::Deny(reason)), _) => Verdict::Deny(reason),
            (Some(floor), _) => floor,
            (None, verdict) => verdict,
        }
    }
}

fn program(words: &[String]) -> Option<String> {
    let mut rest = words.iter().skip_while(|word| assignment(word).is_some());
    let mut first = rest.next()?;
    while first == "env" || first.starts_with('-') || assignment(first).is_some() {
        first = rest.next()?;
    }
    let name = first.trim_matches(|c| c == '"' || c == '\'');
    Some(name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase())
}

fn assignment(word: &str) -> Option<&str> {
    let (name, _) = word.split_once('=')?;
    let mut chars = name.chars();
    let first = chars.next()?;
    let named = (first.is_ascii_alphabetic() || first == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    named.then_some(name)
}

fn assignments(text: &str) -> Vec<&str> {
    text.split(|c: char| c.is_whitespace() || matches!(c, ';' | '&' | '|' | '(' | '`' | '"' | '\''))
        .filter_map(assignment)
        .collect()
}

const HIDDEN: &[(char, char)] = &[
    ('\u{00AD}', '\u{00AD}'),
    ('\u{061C}', '\u{061C}'),
    ('\u{180E}', '\u{180E}'),
    ('\u{200B}', '\u{200F}'),
    ('\u{202A}', '\u{202E}'),
    ('\u{2060}', '\u{2064}'),
    ('\u{2066}', '\u{2069}'),
    ('\u{FEFF}', '\u{FEFF}'),
    ('\u{E0000}', '\u{E007F}'),
];

fn is_hidden(c: char) -> bool {
    HIDDEN.iter().any(|(first, last)| (*first..=*last).contains(&c))
}

pub fn hidden(text: &str) -> Vec<char> {
    let mut found: Vec<char> = text.chars().filter(|c| is_hidden(*c)).collect();
    found.dedup();
    found
}

pub fn revealed(text: &str) -> String {
    text.chars().map(|c| if is_hidden(c) { format!("<U+{:04X}>", u32::from(c)) } else { c.to_string() }).collect()
}

pub fn warned(text: &str) -> String {
    let found = hidden(text);
    if found.is_empty() {
        return text.to_owned();
    }
    let mut codes: Vec<String> = found.iter().map(|c| format!("U+{:04X}", u32::from(*c))).collect();
    codes.sort();
    codes.dedup();
    format!(
        "Warning: this holds invisible or direction-changing characters ({}), so it may read differently from what runs. They are shown as <U+…> below.\n{}",
        codes.join(", "),
        revealed(text)
    )
}
