use std::path::{Path, PathBuf};

use super::{Allowed, Effect, Guard, Remembered};
use crate::PermissionMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Call<'a> {
    pub tool: &'a str,
    pub subject: &'a str,
    pub effects: &'a [Effect],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protected {
    pub name: String,
    pub guard: Guard,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bounds {
    pub roots: Vec<PathBuf>,
    pub readable: Vec<PathBuf>,
    pub protected: Vec<Protected>,
}

impl Bounds {
    pub fn of(roots: Vec<PathBuf>, readable: Vec<PathBuf>) -> Self {
        let protected = vec![
            Protected { name: ".git".to_owned(), guard: Guard::GitFolder },
            Protected { name: brand::DIR.to_owned(), guard: Guard::OwnConfiguration },
        ];
        Self { roots, readable, protected }
    }

    fn root_holding(&self, real: &Path) -> Option<&PathBuf> {
        self.roots.iter().find(|root| real.starts_with(root))
    }

    pub fn may_read(&self, real: &Path) -> bool {
        self.root_holding(real).is_some() || self.readable.iter().any(|folder| real.starts_with(folder))
    }

    pub fn guard(&self, real: &Path) -> Option<Guard> {
        let Some(root) = self.root_holding(real) else { return Some(Guard::OutsideTheProject) };
        let inside = real.strip_prefix(root).ok()?;
        inside.components().find_map(|part| {
            let name = part.as_os_str().to_string_lossy();
            self.protected.iter().find(|protected| name.eq_ignore_ascii_case(&protected.name)).map(|found| found.guard)
        })
    }
}

#[derive(Debug)]
pub struct PolicyContext<'a> {
    pub mode: PermissionMode,
    pub ceiling: Option<PermissionMode>,
    pub remembered: &'a Remembered,
    pub bounds: &'a Bounds,
    pub no_prompts: bool,
    pub allowed: Option<&'a Allowed>,
}

impl PolicyContext<'_> {
    pub fn mode(&self) -> PermissionMode {
        self.mode.capped(self.ceiling)
    }

    pub fn settled(&self, verdict: super::Verdict, tool: &str) -> super::Verdict {
        if self.no_prompts { verdict.without_the_user(tool) } else { verdict }
    }
}
