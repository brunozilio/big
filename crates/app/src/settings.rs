use std::path::PathBuf;

use domain::legacy::OWNER_DIR;
use platform::store::Document;
use platform::sync::Locked;
use serde_json::Value;

const AT_MOST: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("these settings are too large to keep")]
    TooLarge,
    #[error("{0}")]
    Unwritten(String),
    #[error("{0}")]
    Unreadable(String),
}

impl From<SettingsError> for String {
    fn from(error: SettingsError) -> Self {
        error.to_string()
    }
}

fn fits(settings: &Value) -> Result<(), SettingsError> {
    let json = serde_json::to_vec_pretty(settings).map_err(|error| SettingsError::Unwritten(error.to_string()))?;
    if json.len() > AT_MOST {
        return Err(SettingsError::TooLarge);
    }
    Ok(())
}

pub trait SettingsStore: Send + Sync {
    fn load(&self) -> Option<Value>;
    fn save(&self, settings: &Value) -> Result<(), SettingsError>;
}

#[derive(Debug, Clone)]
pub struct FileSettingsStore {
    kept: Document,
}

impl FileSettingsStore {
    pub fn new(root: PathBuf) -> Self {
        Self { kept: Document::at(root.join(OWNER_DIR).join("settings.json")) }
    }
}

impl SettingsStore for FileSettingsStore {
    fn load(&self) -> Option<Value> {
        match self.kept.read() {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(%error, "the settings could not be read");
                None
            }
        }
    }

    fn save(&self, settings: &Value) -> Result<(), SettingsError> {
        fits(settings)?;
        self.kept.write(settings).map_err(|error| SettingsError::Unwritten(error.to_string()))
    }
}

#[derive(Debug, Default)]
pub struct InMemorySettingsStore {
    kept: std::sync::Mutex<Option<Value>>,
}

impl SettingsStore for InMemorySettingsStore {
    fn load(&self) -> Option<Value> {
        self.kept.locked().clone()
    }

    fn save(&self, settings: &Value) -> Result<(), SettingsError> {
        *self.kept.locked() = Some(settings.clone());
        Ok(())
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
