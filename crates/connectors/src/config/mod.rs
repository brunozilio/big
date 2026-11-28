mod schema;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

pub use schema::{EXPORT, applied};

pub const FILE: &str = "config.toml";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Configuration {
    sections: BTreeMap<String, Map<String, Value>>,
}

impl Configuration {
    pub fn built() -> Configuration {
        let mut sections = BTreeMap::new();
        for (name, pairs) in brand::SECTIONS {
            let pairs = pairs.iter().map(|(key, value)| ((*key).to_owned(), Value::from(*value))).collect();
            sections.insert((*name).to_owned(), pairs);
        }
        Configuration { sections }
    }

    pub fn read() -> Configuration {
        files().iter().fold(Configuration::built(), |configuration, file| configuration.with_file(file))
    }

    pub fn with_file(self, file: &Path) -> Configuration {
        match std::fs::read_to_string(file) {
            Ok(text) => self.with_text(&text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => self,
            Err(error) => {
                tracing::warn!(file = %file.display(), %error, "the configuration file could not be read");
                self
            }
        }
    }

    pub fn with_text(mut self, text: &str) -> Configuration {
        let (_, sections) = brand::pairs::values(text);
        for (name, pairs) in sections {
            let section = self.sections.entry(name).or_default();
            section.extend(pairs.into_iter().map(|(key, value)| (key, Value::from(value))));
        }
        self
    }

    pub fn section(&self, name: &str) -> Map<String, Value> {
        self.sections.get(name).cloned().unwrap_or_default()
    }
}

pub fn files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    if content::mode() == content::Mode::Live
        && let Some(repository) = content::live_root().parent()
    {
        files.push(repository.join(FILE));
    }
    files.extend(content::home().map(|home| home.join(FILE)));
    files
}

#[cfg(test)]
mod tests;
