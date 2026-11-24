use std::path::Path;

use crate::tools::{Declared, Effect};

#[derive(Debug, Clone, Copy)]
pub struct Fence<'a> {
    effects: &'a [Effect],
}

impl<'a> Fence<'a> {
    pub fn of(effects: &'a [Effect]) -> Self {
        Self { effects }
    }

    fn anything(&self) -> bool {
        self.effects.contains(&Effect::Declared(Declared::Executes))
    }

    pub fn reads(&self, real: &Path) -> Result<(), String> {
        let covered = self.anything()
            || self.effects.iter().any(|effect| match effect {
                Effect::ReadPath(place) | Effect::WritePath(place) => place.real == real,
                Effect::ReadTree(place) => real.starts_with(&place.real),
                Effect::Declared(Declared::Edits) => true,
                _ => false,
            });
        covered.then_some(()).ok_or_else(|| format!("{} is outside what this call declared it reads", real.display()))
    }

    pub fn writes(&self, real: &Path) -> Result<(), String> {
        let covered = self.anything()
            || self.effects.iter().any(|effect| match effect {
                Effect::WritePath(place) => place.real == real,
                Effect::Declared(Declared::Edits) => true,
                _ => false,
            });
        covered.then_some(()).ok_or_else(|| format!("{} is outside what this call declared it writes", real.display()))
    }

    pub fn runs(&self, command: &str) -> Result<(), String> {
        let covered = self.anything()
            || self.effects.iter().any(|effect| matches!(effect, Effect::Exec(exec) if exec.command.text() == command));
        covered.then_some(()).ok_or_else(|| format!("`{command}` is not the command this call declared it runs"))
    }

    pub fn runs_any(&self) -> Result<(), String> {
        let covered = self.anything() || self.effects.iter().any(|effect| matches!(effect, Effect::Exec(_)));
        covered.then_some(()).ok_or_else(|| "this call did not declare that it runs a command".to_owned())
    }

    pub fn spawns(&self, kind: crate::tools::SpawnKind) -> Result<(), String> {
        let covered = self.anything() || self.effects.contains(&Effect::Spawn(kind));
        covered.then_some(()).ok_or_else(|| format!("this call did not declare that it starts {kind:?}"))
    }

    pub fn reaches(&self, host: &str) -> Result<(), String> {
        let covered = self.anything()
            || self.effects.iter().any(|effect| match effect {
                Effect::Net { host: None } | Effect::Fetch { host: None } => true,
                Effect::Net { host: Some(declared) } | Effect::Fetch { host: Some(declared) } => declared == host,
                _ => false,
            });
        covered.then_some(()).ok_or_else(|| format!("{host} is outside what this call declared it reaches"))
    }
}

#[cfg(test)]
mod tests {
    use domain::permission::Place;

    use super::*;

    #[test]
    fn a_call_reaches_only_what_it_declared() {
        let effects = [Effect::ReadTree(Place::at("/p/src")), Effect::WritePath(Place::at("/p/out.txt"))];
        let fence = Fence::of(&effects);
        assert!(fence.reads(Path::new("/p/src/a.rs")).is_ok());
        assert!(fence.reads(Path::new("/p/out.txt")).is_ok());
        assert!(fence.reads(Path::new("/p/secret")).is_err());
        assert!(fence.writes(Path::new("/p/out.txt")).is_ok());
        assert!(fence.writes(Path::new("/p/src/a.rs")).is_err());
        let edits = [Effect::Declared(Declared::Edits)];
        assert!(Fence::of(&edits).writes(Path::new("/anywhere")).is_ok());
        assert!(Fence::of(&[]).reads(Path::new("/p")).is_err());
    }

    #[test]
    fn a_call_runs_only_the_command_it_declared() {
        let effects = [Effect::exec("ls -la")];
        assert!(Fence::of(&effects).runs("ls -la").is_ok());
        assert!(Fence::of(&effects).runs("rm -rf /").is_err());
        assert!(Fence::of(&effects).runs_any().is_ok());
        assert!(Fence::of(&[]).runs_any().is_err());
        assert!(Fence::of(&[Effect::Declared(Declared::Executes)]).runs("anything").is_ok());
    }

    #[test]
    fn a_call_reaches_only_the_host_it_declared() {
        let effects = [Effect::Net { host: Some("docs.rs".into()) }];
        assert!(Fence::of(&effects).reaches("docs.rs").is_ok());
        assert!(Fence::of(&effects).reaches("evil.dev").is_err());
        assert!(Fence::of(&[Effect::Net { host: None }]).reaches("anywhere.dev").is_ok());
        assert!(Fence::of(&[]).reaches("docs.rs").is_err());
    }
}
