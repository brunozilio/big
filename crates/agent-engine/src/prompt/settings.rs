use crate::scope::Scope;
use serde_json::Value;
use std::path::{Path, PathBuf};
pub const MANAGED: &str = "managed-settings.json";
const LOCAL: &str = "settings.local.json";

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub schema: Option<Value>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub hooks: crate::hooks::Hooks,
    #[serde(default)]
    pub interceptors: crate::intercept::Interceptors,
    #[serde(default)]
    pub escalation: Option<String>,
    #[serde(default)]
    pub fallbacks: Vec<String>,
    #[serde(default)]
    pub small_model: Option<String>,
    #[serde(default)]
    pub mcp_approval: domain::mcp_config::McpApproval,
    #[serde(default)]
    pub lsp_servers: Vec<LspLayer>,
    #[serde(default)]
    pub added_dirs: Vec<String>,
    pub scripts: crate::tools::script_tool::ScriptGrants,
    #[serde(default)]
    pub notices: Vec<String>,
    #[serde(default)]
    pub skipped_files: Vec<PathBuf>,
    #[serde(default)]
    pub bash_output: domain::output::OutputLimit,
}

pub type LspLayer = std::collections::BTreeMap<String, domain::lsp::ServerConfig>;
fn model_name(value: &Value, key: &str) -> Result<Option<String>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let name = value.as_str().filter(|s| !s.is_empty() && !s.contains(char::is_whitespace));
    Ok(Some(name.ok_or(format!("{key} must be a model name"))?.to_owned()))
}
fn model_names(value: &Value, key: &str) -> Result<Vec<String>, String> {
    match value.as_array() {
        Some(models) => models.iter().map(|model| model_name(model, key)).filter_map(Result::transpose).collect(),
        None => Ok(model_name(value, key)?.into_iter().collect()),
    }
}

pub fn load(scope: &Scope) -> Result<Settings, String> {
    let home = scope.home();
    let roots = roots(scope.cwd(), home);
    let mut merged = Settings::default();
    merged.hooks.extend(&crate::hooks::Hooks::gathered(home, &roots, &|file| scope.honours(file)));
    merged.interceptors =
        crate::intercept::Interceptors::gathered(home, scope.cwd(), &roots, &|file| scope.honours(file));
    let mut problems = Vec::new();
    let mut skipped = Vec::new();
    for (path, users) in files(&roots, home) {
        let honoured = users || scope.honours(&path);
        let owners = users || (honoured && path.file_name().is_some_and(|name| name == LOCAL));
        let value = match read(&path) {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(error) if owners => return Err(error),
            Err(error) => {
                skipped.push((path, error));
                continue;
            }
        };
        let checked = crate::config_schema::checked_settings(value);
        problems.extend(checked.located(&path));
        let value = checked.value;
        let layer = Layer { path: &path, value: &value, owners, runs_hooks: honoured };
        if owners {
            layer.merge_into(&mut merged)?;
            continue;
        }
        let mut tried = merged.clone();
        match layer.merge_into(&mut tried) {
            Ok(()) => merged = tried,
            Err(error) => skipped.push((path, error)),
        }
    }
    problems.extend(skipped.iter().map(|(path, error)| skipped_layer(path, error)));
    merged.skipped_files = skipped.into_iter().map(|(path, _)| path).collect();
    merged.notices = Vec::new();
    crate::config_schema::logged_once(&problems);
    merged.notices.extend(problems);
    Ok(merged)
}

fn roots(cwd: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = home {
        roots.push(home.join(brand::DIR));
    }
    let mut ancestors: Vec<_> = cwd.ancestors().collect();
    ancestors.reverse();
    for directory in ancestors {
        roots.push(directory.join(brand::DIR));
    }
    let mut seen = std::collections::HashSet::new();
    roots.retain(|root| seen.insert(root.canonicalize().unwrap_or_else(|_| root.clone())));
    roots
}

fn files(roots: &[PathBuf], home: Option<&Path>) -> Vec<(PathBuf, bool)> {
    let is_user_root = |root: &Path| home.is_some_and(|home| root == home.join(brand::DIR));
    let mut files: Vec<(PathBuf, bool)> = roots
        .iter()
        .flat_map(|root| ["settings.json", LOCAL].map(|name| (root.join(name), is_user_root(root))))
        .collect();
    files.extend(home.map(|home| (home.join(brand::DIR).join(MANAGED), true)));
    files
}

fn skipped_layer(path: &Path, error: &str) -> String {
    let shown = path.display().to_string();
    let why = error.strip_prefix(&shown).and_then(|rest| rest.strip_prefix(": ")).unwrap_or(error);
    format!("Skipped {shown}: {why}; the rest of the settings apply. Fix or remove that file.")
}

fn read(path: &Path) -> Result<Option<Value>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    serde_json::from_str(&text).map(Some).map_err(|e| format!("{}: {e}", path.display()))
}

struct Layer<'a> {
    path: &'a Path,
    value: &'a Value,
    owners: bool,
    runs_hooks: bool,
}

impl Layer<'_> {
    fn merge_into(&self, merged: &mut Settings) -> Result<(), String> {
        self.merge_access(merged)?;
        self.merge_agent(merged)?;
        self.merge_models(merged)?;
        self.merge_extensions(merged)?;
        if let Some(value) = self.value.get("jsonSchema") {
            merged.schema = (!value.is_null()).then(|| value.clone());
        }
        Ok(())
    }

    fn located(&self, error: impl std::fmt::Display) -> String {
        format!("{}: {error}", self.path.display())
    }

    fn merge_access(&self, merged: &mut Settings) -> Result<(), String> {
        let settings = self.value;
        merged.mcp_approval.read(settings);
        super::permissions::read(settings, self.owners, &mut merged.added_dirs).map_err(|e| self.located(e))?;
        if let Some(value) = settings.get("scripts") {
            merged.scripts = merged.scripts.layered(value, self.owners)?;
        }
        Ok(())
    }

    fn merge_agent(&self, merged: &mut Settings) -> Result<(), String> {
        if let Some(value) = self.value.get("bashOutputLimit") {
            let characters = value.as_u64().ok_or("bashOutputLimit must be a whole number of characters")?;
            merged.bash_output = domain::output::OutputLimit::of(characters);
        }
        if let Some(value) = self.value.get("agent") {
            merged.agent = if value.is_null() {
                None
            } else {
                Some(value.as_str().ok_or("agent setting must be a name")?.to_owned())
            };
        }
        Ok(())
    }

    fn merge_models(&self, merged: &mut Settings) -> Result<(), String> {
        let settings = self.value;
        if self.owners
            && let Some(value) = settings.get("escalationModel")
        {
            merged.escalation = model_name(value, "escalationModel")?;
        }
        if let Some(value) = settings.get("fallbackModel") {
            merged.fallbacks = model_names(value, "fallbackModel")?;
        }
        if let Some(value) = settings.get("smallModel") {
            merged.small_model = model_name(value, "smallModel")?;
        }
        Ok(())
    }

    fn merge_extensions(&self, merged: &mut Settings) -> Result<(), String> {
        if let Some(value) = self.value.get("hooks").filter(|_| self.runs_hooks) {
            let hooks = crate::hooks::Hooks::parse(value).map_err(|e| self.located(format!("hooks: {e}")))?;
            merged.hooks.extend(&hooks);
        }
        if let Some(value) = self.value.get("lspServers") {
            let layer: LspLayer =
                serde_json::from_value(value.clone()).map_err(|e| self.located(format!("lspServers: {e}")))?;
            merged.lsp_servers.push(if self.owners {
                layer
            } else {
                layer.into_iter().map(|(server, config)| (server, config.without_commands())).collect()
            });
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn load_trusted(cwd: &Path, home: Option<&Path>) -> Result<Settings, String> {
    load(&Scope::at(cwd, home))
}
#[cfg(test)]
#[path = "settings_lsp_tests.rs"]
mod lsp_tests;
#[cfg(test)]
#[path = "settings_schema_tests.rs"]
mod schema_tests;
#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
