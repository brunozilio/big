use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

pub type Builtin = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Repo,
    Factory,
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
    chain: Vec<String>,
    user: Option<PathBuf>,
    builtin: Builtin,
    listed: Vec<String>,
}

pub const FACTORIES: &str = "factories";

pub const FACTORY_FILE: &str = "factory";

const MOST_ANCESTORS: usize = 16;

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
        let (layers, chain) = layered(repo_dir.as_deref(), user_dir.as_deref());
        Resources { layers, chain, user: user_dir, builtin, listed: Vec::new() }
    }

    #[must_use]
    pub fn in_repo(&self, repo: &Path) -> Resources {
        let (layers, chain) = layered(Some(&repo.join(brand::DIR)), self.user.as_deref());
        Resources {
            layers,
            chain,
            user: self.user.clone(),
            builtin: Arc::clone(&self.builtin),
            listed: self.listed.clone(),
        }
    }

    pub fn chain(&self) -> &[String] {
        &self.chain
    }

    pub fn factory_dir(&self, name: &str) -> Option<PathBuf> {
        self.user.as_ref().map(|user| user.join(FACTORIES).join(name))
    }

    pub fn repo_dir(&self) -> Option<&Path> {
        self.layers.iter().find(|(origin, _)| *origin == Origin::Repo).map(|(_, dir)| dir.as_path())
    }

    pub fn layer_dirs(&self) -> impl Iterator<Item = (Origin, &Path)> {
        self.layers.iter().map(|(origin, dir)| (*origin, dir.as_path()))
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

fn layered(repo: Option<&Path>, user: Option<&Path>) -> (Vec<(Origin, PathBuf)>, Vec<String>) {
    let mut layers: Vec<(Origin, PathBuf)> = repo.map(|dir| (Origin::Repo, dir.to_path_buf())).into_iter().collect();
    let mut chain: Vec<String> = Vec::new();
    if let Some(user) = user {
        let mut next = repo.and_then(named).or_else(|| named(user));
        while let Some(name) = next.take() {
            if chain.contains(&name) || chain.len() >= MOST_ANCESTORS {
                break;
            }
            let place = user.join(FACTORIES).join(&name);
            next = named(&place);
            layers.push((Origin::Factory, place));
            chain.push(name);
        }
        layers.push((Origin::User, user.to_path_buf()));
    }
    (layers, chain)
}

fn named(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(FACTORY_FILE)).ok()?;
    let name = text.trim();
    let plain = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    (plain && !name.starts_with('.')).then(|| name.to_owned())
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
