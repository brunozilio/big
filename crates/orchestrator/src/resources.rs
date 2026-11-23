use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

pub type Builtin = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Repo,
    User,
    Builtin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub path: String,
    pub origin: Origin,
    pub text: String,
}

#[derive(Clone)]
pub struct Resources {
    layers: Vec<(Origin, PathBuf)>,
    builtin: Builtin,
    listed: Vec<String>,
}

impl fmt::Debug for Resources {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Resources").field("layers", &self.layers).finish_non_exhaustive()
    }
}

const SCRIPT_EXTENSIONS: [&str; 2] = ["ts", "js"];

const OVERRIDABLE: [&str; 2] = ["steps/", "views/"];

impl Resources {
    pub fn standard(repo: Option<&Path>) -> Resources {
        content::install_once();
        let builtin: Builtin = Arc::new(content::builtin_text);
        let listed = content::builtin_files("").into_iter().map(|(path, _)| path.to_owned()).collect();
        Resources::new(repo.map(|repo| repo.join(brand::DIR)), content::folder(), builtin).listing(listed)
    }

    pub fn new(repo_dir: Option<PathBuf>, user_dir: Option<PathBuf>, builtin: Builtin) -> Resources {
        let layers = [(Origin::Repo, repo_dir), (Origin::User, user_dir)]
            .into_iter()
            .filter_map(|(origin, dir)| dir.map(|dir| (origin, dir)))
            .collect();
        Resources { layers, builtin, listed: Vec::new() }
    }

    #[must_use]
    pub fn listing(mut self, builtin_paths: Vec<String>) -> Resources {
        self.listed = builtin_paths;
        self
    }

    pub fn workflows(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let mut add = |file: &str| {
            let Some(name) = file.strip_suffix(".ts").or_else(|| file.strip_suffix(".js")) else { return };
            if !name.contains('/') && !name.ends_with(".d") && !names.iter().any(|known| known == name) {
                names.push(name.to_owned());
            }
        };
        for (_, dir) in &self.layers {
            let Ok(entries) = std::fs::read_dir(dir.join("workflows")) else { continue };
            for entry in entries.flatten() {
                add(&entry.file_name().to_string_lossy());
            }
        }
        self.listed.iter().filter_map(|path| path.strip_prefix("workflows/")).for_each(&mut add);
        names
    }

    pub fn find(&self, path: &str) -> Option<Resource> {
        let alias = path.strip_prefix("sdk/").filter(|rest| OVERRIDABLE.iter().any(|dir| rest.starts_with(dir)));
        for (origin, dir) in &self.layers {
            for candidate in [Some(path), alias].into_iter().flatten() {
                let Some(file) = content::override_path(dir, candidate) else { continue };
                if let Ok(text) = std::fs::read_to_string(file) {
                    let text = brand::text(text.trim_end_matches('\n'));
                    return Some(Resource { path: path.to_owned(), origin: *origin, text });
                }
            }
        }
        (self.builtin)(path).map(|text| Resource { path: path.to_owned(), origin: Origin::Builtin, text })
    }

    pub fn script(&self, base: &str) -> Option<Resource> {
        script_candidates(base).iter().find_map(|candidate| self.find(candidate))
    }

    pub fn agent(&self, name: &str) -> Option<Resource> {
        self.find(&format!("agents/{name}.md"))
    }

    pub fn workflow(&self, name: &str) -> Option<Resource> {
        self.script(&format!("workflows/{name}"))
    }
}

pub fn script_candidates(base: &str) -> Vec<String> {
    let named = Path::new(base).extension().is_some_and(|ext| ext == "ts" || ext == "js" || ext == "mjs");
    if named {
        return vec![base.to_owned()];
    }
    let mut candidates: Vec<String> = SCRIPT_EXTENSIONS.iter().map(|ext| format!("{base}.{ext}")).collect();
    candidates.extend(SCRIPT_EXTENSIONS.iter().map(|ext| format!("{base}/index.{ext}")));
    candidates
}
