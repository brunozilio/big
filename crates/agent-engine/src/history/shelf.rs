use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::{Hit, Query, Read, Session, Turn};
use crate::sessions::Told;

const LOG: &str = ".jsonl";
const TRACE: &str = ".trace.jsonl";
const SAVED: &str = ".json";
const AROUND: usize = 120;
const HITS_PER_SESSION: usize = 3;
const TURN_AT_MOST: usize = 4000;
const READ_AT_MOST: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub name: String,
    pub dir: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Named {
    pub title: Option<String>,
    pub cwd: String,
}

#[derive(Debug, Clone, Default)]
pub struct Shelf {
    kept: Vec<Kept>,
    named: HashMap<String, Named>,
}

#[derive(Debug, Clone)]
struct Stored {
    id: String,
    kept_in: String,
    logs: Vec<PathBuf>,
    saved: Option<PathBuf>,
    updated_ms: u64,
}

fn session_id(name: &str) -> Option<(&str, bool)> {
    let (stem, log) = if let Some(stem) = name.strip_suffix(TRACE) {
        (stem, true)
    } else if let Some(stem) = name.strip_suffix(LOG) {
        (stem, true)
    } else {
        (name.strip_suffix(SAVED)?, false)
    };
    let hashed = stem.len() == 64 && stem.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    hashed.then_some((stem, log))
}

fn modified_ms(path: &Path) -> u64 {
    let modified = std::fs::metadata(path).and_then(|meta| meta.modified()).ok();
    let since = modified.and_then(|at| at.duration_since(UNIX_EPOCH).ok());
    since.map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}

fn stored_in(kept: &Kept) -> Vec<Stored> {
    let Ok(entries) = std::fs::read_dir(&kept.dir) else { return Vec::new() };
    let mut found: BTreeMap<String, Stored> = BTreeMap::new();
    for path in entries.flatten().map(|entry| entry.path()) {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else { continue };
        let Some((id, log)) = session_id(name) else { continue };
        let stored = found.entry(id.to_owned()).or_insert_with(|| Stored {
            id: id.to_owned(),
            kept_in: kept.name.clone(),
            logs: Vec::new(),
            saved: None,
            updated_ms: 0,
        });
        stored.updated_ms = stored.updated_ms.max(modified_ms(&path));
        if log { stored.logs.push(path) } else { stored.saved = Some(path) }
    }
    found.into_values().collect()
}

fn told(stored: &Stored) -> Told {
    let mut told = Told::default();
    let mut logs = stored.logs.clone();
    logs.sort();
    for log in &logs {
        if let Ok(text) = std::fs::read_to_string(log) {
            told.add_log(&text);
        }
    }
    if let Some(text) = stored.saved.as_ref().and_then(|saved| std::fs::read_to_string(saved).ok()) {
        told.add_saved(&text);
    }
    told
}

fn folded(text: &str) -> Vec<char> {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

fn find(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn excerpt(text: &str, words: &[Vec<char>]) -> Option<String> {
    let chars: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let plain: Vec<char> = text.chars().collect();
    let haystack = folded(text);
    let mut first = None;
    for word in words {
        let at = find(&haystack, word)?;
        first = Some(first.map_or(at, |known: usize| known.min(at)));
    }
    let at = first?;
    let shown = if chars.len() == plain.len() { &plain } else { &chars };
    let start = at.saturating_sub(AROUND);
    let end = (at + AROUND).min(shown.len());
    let body: String = shown[start..end].iter().collect();
    let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let before = if start > 0 { "…" } else { "" };
    let after = if end < shown.len() { "…" } else { "" };
    Some(format!("{before}{body}{after}"))
}

fn clipped(text: &str) -> String {
    match text.char_indices().nth(TURN_AT_MOST) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

fn role(user: bool) -> &'static str {
    if user { "user" } else { "assistant" }
}

impl Shelf {
    pub fn new(kept: Vec<Kept>) -> Self {
        Self { kept, named: HashMap::new() }
    }

    pub fn named(mut self, id: String, title: Option<String>, cwd: String) -> Self {
        self.named.insert(id, Named { title, cwd });
        self
    }

    fn stored(&self) -> Vec<Stored> {
        let mut seen = HashSet::new();
        let mut all: Vec<Stored> =
            self.kept.iter().flat_map(stored_in).filter(|stored| seen.insert(stored.id.clone())).collect();
        all.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms).then_with(|| a.id.cmp(&b.id)));
        all
    }

    fn session(&self, stored: &Stored, told: &Told) -> Session {
        let named = self.named.get(&stored.id);
        Session {
            session: stored.id.clone(),
            kept_in: stored.kept_in.clone(),
            title: named.and_then(|named| named.title.clone()),
            cwd: named.map(|named| named.cwd.clone()).or_else(|| told.cwd.clone()),
            updated_ms: stored.updated_ms,
        }
    }

    fn matching(&self, query: &Query) -> impl Iterator<Item = (Session, Told)> + '_ {
        let since = query.since_ms.unwrap_or(0);
        let cwd = query.cwd.clone().filter(|cwd| !cwd.is_empty());
        self.stored().into_iter().filter(move |stored| stored.updated_ms >= since).filter_map(move |stored| {
            let told = told(&stored);
            let session = self.session(&stored, &told);
            let inside = cwd.as_ref().is_none_or(|cwd| session.cwd.as_ref().is_some_and(|at| at.starts_with(cwd)));
            inside.then_some((session, told))
        })
    }

    pub fn sessions(&self, query: &Query) -> Vec<Session> {
        self.matching(query).map(|(session, _)| session).take(query.limit.max(1)).collect()
    }

    pub fn search(&self, query: &Query) -> Vec<Hit> {
        let words: Vec<Vec<char>> = query.text.split_whitespace().map(folded).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for (session, told) in self.matching(query) {
            hits.extend(hits_in(&session, &told, &words).into_iter().take(HITS_PER_SESSION));
            if hits.len() >= query.limit.max(1) {
                break;
            }
        }
        hits.truncate(query.limit.max(1));
        hits
    }

    pub fn read(&self, session: &str, from: usize, count: usize) -> Result<Read, String> {
        let wanted = session.trim();
        if wanted.len() < 8 {
            return Err("give the session id, or at least its first 8 characters".into());
        }
        let found: Vec<Stored> = self.stored().into_iter().filter(|stored| stored.id.starts_with(wanted)).collect();
        let [stored] = found.as_slice() else {
            return Err(if found.is_empty() {
                format!("no saved session starts with {wanted}")
            } else {
                format!("{} sessions start with {wanted}; give more of the id", found.len())
            });
        };
        let told = told(stored);
        let turns: Vec<(bool, String)> = if told.turns.is_empty() {
            told.prompts.iter().map(|prompt| (true, prompt.text.clone())).collect()
        } else {
            told.turns.iter().map(|said| (said.user, said.text.clone())).collect()
        };
        let total = turns.len();
        let shown = turns
            .into_iter()
            .enumerate()
            .skip(from)
            .take(count.clamp(1, READ_AT_MOST))
            .map(|(turn, (user, text))| Turn { turn, role: role(user), text: clipped(&text) })
            .collect();
        Ok(Read { session: self.session(stored, &told), total, turns: shown })
    }
}

fn hits_in(session: &Session, told: &Told, words: &[Vec<char>]) -> Vec<Hit> {
    let hit = |user: bool, at_ms: Option<u64>, turn: Option<usize>, excerpt: String| Hit {
        session: session.clone(),
        role: role(user),
        at_ms,
        turn,
        excerpt,
    };
    let mut hits = Vec::new();
    let prompted: HashSet<&str> = told.prompts.iter().map(|prompt| prompt.text.trim()).collect();
    for prompt in &told.prompts {
        let Some(shown) = excerpt(&prompt.text, words) else { continue };
        let turn = told.turns.iter().position(|said| said.user && said.text.trim() == prompt.text.trim());
        hits.push(hit(true, Some(prompt.at_ms), turn, shown));
    }
    for (turn, said) in told.turns.iter().enumerate() {
        if said.user && prompted.contains(said.text.trim()) {
            continue;
        }
        let Some(shown) = excerpt(&said.text, words) else { continue };
        hits.push(hit(said.user, None, Some(turn), shown));
    }
    hits
}

#[cfg(test)]
#[path = "shelf_tests.rs"]
mod tests;
