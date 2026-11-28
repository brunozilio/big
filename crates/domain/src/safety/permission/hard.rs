use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub use super::env::EnvRule;
use super::env;
use super::hidden::{hidden, revealed};
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

    fn denies_writes(self) -> bool {
        matches!(self, Self::Credentials | Self::SandboxEscape)
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
    pub secrets: Vec<String>,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub apps: Vec<String>,
    #[serde(default)]
    pub env: Vec<EnvRule>,
    #[serde(default)]
    pub carriers: Vec<String>,
}

pub type Declared = BTreeMap<Category, Patterns>;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    Under(PathBuf),
    Named(String),
    Homeless(PathBuf),
}

impl Place {
    fn holds(&self, real: &Path, inside: &Path) -> bool {
        match self {
            Self::Under(place) => real.starts_with(place),
            Self::Named(name) => inside.components().any(|part| glob(name, &part.as_os_str().to_string_lossy())),
            Self::Homeless(rest) => {
                let parts: Vec<_> = real.components().collect();
                let wanted: Vec<_> = rest.components().collect();
                !wanted.is_empty() && parts.windows(wanted.len()).any(|window| window == wanted.as_slice())
            }
        }
    }
}

fn glob(rule: &str, name: &str) -> bool {
    match rule.split_once('*') {
        Some((head, tail)) => name.len() >= head.len() + tail.len() && name.starts_with(head) && name.ends_with(tail),
        None => rule.eq_ignore_ascii_case(name),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Floor {
    home: Option<PathBuf>,
    roots: Vec<PathBuf>,
    paths: Vec<(Category, Place)>,
    secrets: Vec<Place>,
    hosts: Vec<(Category, String)>,
    apps: Vec<(Category, String)>,
    env: Vec<(Category, EnvRule)>,
    carriers: Vec<(String, String)>,
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
        self.secrets.extend(other.secrets);
        self.hosts.extend(other.hosts);
        self.apps.extend(other.apps);
        self.env.extend(other.env);
        self.carriers.extend(other.carriers);
    }

    pub fn govern(&mut self, places: impl IntoIterator<Item = PathBuf>, named: impl IntoIterator<Item = String>) {
        self.paths.extend(places.into_iter().map(|place| (Category::Governance, Place::Under(place))));
        self.paths.extend(named.into_iter().map(|name| (Category::Governance, Place::Named(name))));
    }

    pub fn add(&mut self, declared: &Declared, settle: &dyn Fn(&Path) -> PathBuf) -> Vec<String> {
        let mut homeless = Vec::new();
        for (category, patterns) in declared {
            for written in patterns.paths.iter().map(|path| path.trim()).filter(|path| !path.is_empty()) {
                self.paths.push((*category, self.placed(written, settle, &mut homeless)));
            }
            for written in patterns.secrets.iter().map(|path| path.trim()).filter(|path| !path.is_empty()) {
                let place = self.placed(written, settle, &mut homeless);
                self.secrets.push(place);
            }
            let lower = |text: &String| text.trim().to_ascii_lowercase();
            self.hosts.extend(patterns.hosts.iter().map(lower).filter(|h| !h.is_empty()).map(|h| (*category, h)));
            self.apps.extend(patterns.apps.iter().map(lower).filter(|a| !a.is_empty()).map(|a| (*category, a)));
            self.env.extend(patterns.env.iter().filter(|rule| rule.named()).map(|rule| (*category, rule.clone())));
            self.carriers.extend(patterns.carriers.iter().filter_map(|carrier| {
                let (app, flag) = carrier.trim().split_once(' ')?;
                Some((app.to_ascii_lowercase(), flag.trim().to_owned()))
            }));
        }
        homeless
    }

    fn placed(&self, written: &str, settle: &dyn Fn(&Path) -> PathBuf, homeless: &mut Vec<String>) -> Place {
        if let Some(place) = self.expanded(written) {
            return Place::Under(settle(&place));
        }
        match home_relative(written) {
            Some(rest) => {
                homeless.push(written.to_owned());
                Place::Homeless(PathBuf::from(rest))
            }
            None => Place::Named(written.to_owned()),
        }
    }

    fn expanded(&self, written: &str) -> Option<PathBuf> {
        let written = written.trim_matches(|c| c == '"' || c == '\'');
        if let Some(rest) = home_relative(written) {
            return self.home.as_ref().map(|home| home.join(rest));
        }
        if written == "~" {
            return self.home.clone();
        }
        Path::new(written).is_absolute().then(|| PathBuf::from(written))
    }

    fn inside<'p>(&self, real: &'p Path) -> &'p Path {
        self.roots.iter().find_map(|root| real.strip_prefix(root).ok()).unwrap_or(real)
    }

    fn path_hit(&self, real: &Path, wanted: impl Fn(Category) -> bool) -> Option<Category> {
        let inside = self.inside(real);
        self.paths.iter().find(|(category, place)| wanted(*category) && place.holds(real, inside)).map(|(c, _)| *c)
    }

    fn secret(&self, real: &Path) -> bool {
        let inside = self.inside(real);
        let named = |name: &str| real.file_name().is_some_and(|file| glob(name, &file.to_string_lossy()));
        self.secrets.iter().any(|place| match place {
            Place::Named(name) => named(name),
            other => other.holds(real, inside),
        })
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

    fn word_hits(&self, word: &str) -> Vec<Hit> {
        let pieces = std::iter::once(word).chain(word.split([':', '=']));
        let pieces: Vec<&str> = pieces.map(|piece| piece.trim_matches(|c| c == '"' || c == '\'')).collect();
        let places: Vec<PathBuf> = pieces
            .iter()
            .filter_map(|piece| self.expanded(piece).or_else(|| home_relative(piece).map(PathBuf::from)))
            .collect();
        let named_secret = pieces.iter().any(|piece| {
            Path::new(piece).file_name().is_some_and(|file| {
                let file = file.to_string_lossy();
                self.secrets.iter().any(|place| matches!(place, Place::Named(name) if glob(name, &file)))
            })
        });
        if named_secret || places.iter().any(|place| self.secret(place)) {
            return vec![Hit::of(Category::Credentials, word, false)];
        }
        places
            .iter()
            .find_map(|place| self.path_hit(place, |_| true))
            .map(|category| Hit::of(category, word, false))
            .into_iter()
            .collect()
    }

    fn exec_hits(&self, simple: &[&str]) -> Vec<Hit> {
        let mut hits = Vec::new();
        for command in simple {
            let parsed = SimpleCommand::of(command);
            let words: Vec<String> = parsed
                .as_ref()
                .map_or_else(|| command.split_whitespace().map(str::to_owned).collect(), |parsed| parsed.words.clone());
            let targets = parsed.iter().flat_map(|parsed| parsed.redirections.iter().map(|r| r.target.clone()));
            hits.extend(words.iter().cloned().chain(targets).flat_map(|word| self.word_hits(&word)));
            hits.extend(env::assigned(&words, &self.carriers).into_iter().filter_map(|set| {
                let (category, _) = self.env.iter().find(|(_, rule)| rule.injects(&set))?;
                Some(Hit::of(*category, format!("{}={}", set.name, set.value), false))
            }));
            if let Some((_, program)) = env::program(&words)
                && let Some((category, _)) = self.apps.iter().find(|(_, app)| *app == program)
            {
                hits.push(Hit::of(*category, program, false));
            }
        }
        hits
    }

    fn place_hits(&self, place: &super::Place, writes: bool) -> Option<Hit> {
        let shown = place.written.display().to_string();
        if self.secret(&place.real) {
            return Some(Hit::of(Category::Credentials, shown, true));
        }
        let category =
            if writes { self.path_hit(&place.real, |_| true) } else { self.path_hit(&place.real, Category::on_reads) }?;
        Some(Hit::of(category, shown, writes && category.denies_writes()))
    }

    fn effect_hits(&self, effect: &Effect) -> Vec<Hit> {
        match effect {
            Effect::ReadPath(place) | Effect::ReadTree(place) => self.place_hits(place, false).into_iter().collect(),
            Effect::WritePath(place) => self.place_hits(place, true).into_iter().collect(),
            Effect::Exec(exec) => {
                let command = &exec.command;
                let whole = [command.text()];
                self.exec_hits(&if command.opaque() { whole.to_vec() } else { command.commands() })
            }
            Effect::Net { host: Some(host) } | Effect::Fetch { host: Some(host) } => self
                .host_hit(host)
                .map(|category| Hit::of(category, host.clone(), category.on_reads()))
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

fn home_relative(written: &str) -> Option<&str> {
    ["~/", "$HOME/", "${HOME}/"].iter().find_map(|prefix| written.strip_prefix(prefix))
}
