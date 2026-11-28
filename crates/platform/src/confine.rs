use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};

pub type People = Arc<dyn Fn(&str) -> io::Result<u32> + Send + Sync>;

pub const SYSTEM: &str = "";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ceiling {
    pub cpu_seconds: u64,
    pub memory_bytes: u64,
    pub processes: u64,
    pub file_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    pub first_uid: u32,
    pub uids: u32,
    pub homes: PathBuf,
    pub kept: Vec<String>,
    pub ceiling: Ceiling,
}

impl Rules {
    pub fn uid(&self, number: u32) -> Option<u32> {
        (number < self.uids).then(|| self.first_uid.checked_add(number)).flatten()
    }

    pub fn holds(&self, uid: u32) -> bool {
        uid >= self.first_uid && uid - self.first_uid < self.uids
    }
}

struct Active {
    rules: Rules,
    people: People,
}

static ACTIVE: OnceLock<Active> = OnceLock::new();

pub fn activate(rules: Rules, people: People) -> Result<(), String> {
    crate::system::may_confine()?;
    std::fs::create_dir_all(&rules.homes).map_err(|error| format!("{}: {error}", rules.homes.display()))?;
    crate::system::set_mode(&rules.homes, 0o711).map_err(|error| format!("{}: {error}", rules.homes.display()))?;
    crate::system::narrow_umask();
    ACTIVE.set(Active { rules, people }).map_err(|_| "the agents are already confined".to_owned())
}

pub fn active() -> bool {
    ACTIVE.get().is_some()
}

pub fn person(owner: Option<&str>) -> io::Result<Option<u32>> {
    let Some(active) = ACTIVE.get() else { return Ok(None) };
    let number = (active.people)(owner.unwrap_or(SYSTEM))?;
    let uid = active.rules.uid(number).ok_or_else(|| io::Error::other("there are more people than agent users"))?;
    Ok(Some(uid))
}

pub fn as_owner(command: &mut Command, owner: Option<&str>) -> io::Result<()> {
    if let Some(uid) = person(owner)? {
        as_uid(command, uid);
    }
    Ok(())
}

pub fn as_owner_of(command: &mut Command, dir: &Path) {
    if let Some(uid) = owner_of(dir) {
        as_uid(command, uid);
    }
}

pub fn confined_in(command: &mut Command, dir: &Path) -> io::Result<()> {
    if !active() {
        return Ok(());
    }
    match owner_of(dir) {
        Some(uid) => {
            as_uid(command, uid);
            Ok(())
        }
        None => as_owner(command, None),
    }
}

pub fn belongs_to(path: &Path, owner: Option<&str>) -> io::Result<bool> {
    match person(owner)? {
        Some(uid) => Ok(crate::system::owner(path)? == uid),
        None => Ok(true),
    }
}

pub fn give_tree(dir: &Path, owner: Option<&str>) -> io::Result<()> {
    let Some(uid) = person(owner)? else { return Ok(()) };
    let mut waiting = vec![dir.to_path_buf()];
    while let Some(at) = waiting.pop() {
        let found = std::fs::symlink_metadata(&at)?;
        crate::system::give(&at, uid)?;
        if found.is_dir() {
            for entry in std::fs::read_dir(&at)? {
                waiting.push(entry?.path());
            }
        }
    }
    Ok(())
}

pub fn owner_of(path: &Path) -> Option<u32> {
    let active = ACTIVE.get()?;
    let uid = path.ancestors().find_map(|at| crate::system::owner(at).ok())?;
    active.rules.holds(uid).then_some(uid)
}

pub fn dir_for(dir: &Path, owner: Option<&str>) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let Some(uid) = person(owner)? else { return Ok(()) };
    crate::system::give(dir, uid)?;
    crate::system::set_mode(dir, 0o700)
}

pub fn shared_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    if active() { crate::system::set_mode(dir, 0o1733) } else { Ok(()) }
}

pub fn passable_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    if active() { crate::system::set_mode(dir, 0o711) } else { Ok(()) }
}

pub fn open_to_all(dir: &Path) -> io::Result<()> {
    if !active() || !dir.exists() {
        return Ok(());
    }
    let mut waiting = vec![dir.to_path_buf()];
    while let Some(at) = waiting.pop() {
        crate::system::open_entry(&at)?;
        if std::fs::symlink_metadata(&at)?.is_dir() {
            for entry in std::fs::read_dir(&at)? {
                waiting.push(entry?.path());
            }
        }
    }
    dir.ancestors().skip(1).filter(|at| at.parent().is_some()).try_for_each(crate::system::passable)
}

pub fn dirs_like_ancestor(dir: &Path) -> io::Result<()> {
    let Some(existing) = dir.ancestors().find(|at| at.exists()) else { return std::fs::create_dir_all(dir) };
    let owner = owner_of(existing);
    let created: Vec<&Path> = dir.ancestors().take_while(|at| *at != existing).collect();
    std::fs::create_dir_all(dir)?;
    match owner {
        Some(uid) => created.iter().try_for_each(|at| crate::system::give(at, uid)),
        None => Ok(()),
    }
}

pub fn give_like_parent(path: &Path) -> io::Result<()> {
    match path.parent().and_then(owner_of) {
        Some(uid) => crate::system::give(path, uid),
        None => Ok(()),
    }
}

fn as_uid(command: &mut Command, uid: u32) {
    let Some(active) = ACTIVE.get() else { return };
    let home = active.rules.homes.join(uid.to_string());
    if let Err(error) = made_home(&home, uid) {
        tracing::warn!(%error, "the home of agent user {uid} could not be made");
    }
    let given: Vec<(OsString, Option<OsString>)> =
        command.get_envs().map(|(name, value)| (name.to_owned(), value.map(ToOwned::to_owned))).collect();
    command.env_clear();
    for name in &active.rules.kept {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for (name, value) in given {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    command.env("HOME", &home).env("USER", format!("agent{uid}")).env("TMPDIR", home.join("tmp"));
    crate::system::switch_to(command, uid, active.rules.ceiling);
}

fn made_home(home: &Path, uid: u32) -> io::Result<()> {
    let scratch = home.join("tmp");
    if !scratch.is_dir() {
        std::fs::create_dir_all(&scratch)?;
        for at in [home, scratch.as_path()] {
            crate::system::give(at, uid)?;
            crate::system::set_mode(at, 0o700)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        Rules { first_uid: 200_000, uids: 10, homes: PathBuf::new(), kept: Vec::new(), ceiling: Ceiling::default() }
    }

    #[test]
    fn a_number_maps_to_a_user_only_inside_the_range() {
        let rules = rules();
        assert_eq!(rules.uid(0), Some(200_000));
        assert_eq!(rules.uid(9), Some(200_009));
        assert_eq!(rules.uid(10), None);
        assert!(rules.holds(200_009));
        assert!(!rules.holds(199_999));
        assert!(!rules.holds(200_010));
        assert!(!rules.holds(0));
    }

    #[test]
    fn without_confinement_nothing_changes() {
        let mut command = Command::new("true");
        command.env("KEEP", "1");
        as_owner(&mut command, Some("ana")).expect("no confinement");
        confined_in(&mut command, Path::new("/")).expect("no confinement");
        assert_eq!(person(Some("ana")).expect("asked"), None);
        assert_eq!(owner_of(Path::new("/")), None);
        assert!(command.get_envs().any(|(name, _)| name == "KEEP"));
    }
}
