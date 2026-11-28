use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use platform::confine::{Ceiling, Rules};
use store::Store;

pub const USER: &str = "user";
pub const WORKSPACES: &str = "workspaces";
pub const WORKTREES: &str = "worktrees";
const LOWEST_UID: u32 = 1000;

pub fn wanted() -> bool {
    brand::isolation::MODE.trim() == USER
}

pub fn prepare(data: &Path, store: &Arc<Store>) -> Result<(), String> {
    if !wanted() {
        return Ok(());
    }
    let rules = configured(data);
    if rules.first_uid < LOWEST_UID || rules.uids == 0 || rules.first_uid.checked_add(rules.uids).is_none() {
        return Err(format!("[isolation] needs first_uid of at least {LOWEST_UID} and a number of uids that fits"));
    }
    let open = [WORKSPACES, WORKTREES].map(|name| data.join(name));
    let kept: Vec<PathBuf> = open.iter().cloned().chain([rules.homes.clone()]).collect();
    let store = Arc::clone(store);
    let people: platform::confine::People = Arc::new(move |owner: &str| {
        let number = store.agent_number(owner).map_err(std::io::Error::other)?;
        u32::try_from(number).map_err(std::io::Error::other)
    });
    platform::confine::activate(rules, people)?;
    closed(data, &kept).map_err(|error| format!("{}: {error}", data.display()))?;
    let [workspaces, worktrees] = open;
    let made = platform::confine::passable_dir(&workspaces).and_then(|()| platform::confine::shared_dir(&worktrees));
    made.map_err(|error| error.to_string())?;
    adopted(&workspaces, &worktrees).map_err(|error| error.to_string())?;
    tracing::info!("each person's agents run as their own user");
    Ok(())
}

fn adopted(workspaces: &Path, worktrees: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(workspaces)? {
        let path = entry?.path();
        let owner = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        if platform::confine::owner_of(&path).is_none() && path.is_dir() {
            platform::confine::give_tree(&path, Some(&owner))?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    for entry in std::fs::read_dir(worktrees)? {
        let path = entry?.path();
        if platform::confine::owner_of(&path).is_none() {
            closed_entry(&path)?;
        }
    }
    Ok(())
}

fn closed_entry(path: &Path) -> std::io::Result<()> {
    let found = std::fs::symlink_metadata(path)?;
    if found.file_type().is_symlink() {
        return Ok(());
    }
    let mode = found.permissions().mode();
    if mode & 0o077 != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & !0o077))?;
    }
    Ok(())
}

fn number(text: &str) -> u64 {
    text.trim().parse().unwrap_or(0)
}

fn configured(data: &Path) -> Rules {
    let section = &brand::isolation::PAIRS;
    let value = |key: &str| section.iter().find(|(name, _)| *name == key).map_or("", |(_, value)| *value);
    let homes = Path::new(value("homes").trim());
    let mebibytes = |key: &str| number(value(key)).saturating_mul(1 << 20);
    Rules {
        first_uid: u32::try_from(number(value("first_uid"))).unwrap_or(0),
        uids: u32::try_from(number(value("uids"))).unwrap_or(0),
        homes: if homes.is_absolute() { homes.to_path_buf() } else { data.join(homes) },
        kept: value("kept_env").split(',').map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).collect(),
        ceiling: Ceiling {
            cpu_seconds: number(value("cpu_seconds")),
            memory_bytes: mebibytes("memory_mb"),
            processes: number(value("processes")),
            file_bytes: mebibytes("file_mb"),
        },
    }
}

fn closed(data: &Path, kept: &[PathBuf]) -> std::io::Result<()> {
    std::fs::set_permissions(data, std::fs::Permissions::from_mode(0o711))?;
    let mut waiting = vec![data.to_path_buf()];
    while let Some(dir) = waiting.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            let found = std::fs::symlink_metadata(&path)?;
            if found.file_type().is_symlink() || kept.contains(&path) {
                continue;
            }
            let mode = found.permissions().mode();
            if mode & 0o007 != 0 {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode & !0o007))?;
            }
            if found.is_dir() {
                waiting.push(path);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_folder_stops_being_readable_by_others_except_the_shared_folders() -> std::io::Result<()> {
        let data = tempfile::tempdir()?;
        let secret = data.path().join("nested").join("app.db");
        std::fs::create_dir_all(secret.parent().ok_or_else(|| std::io::Error::other("parent"))?)?;
        std::fs::write(&secret, "x")?;
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o644))?;
        let shared = data.path().join(WORKSPACES);
        std::fs::create_dir_all(&shared)?;
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o755))?;
        closed(data.path(), std::slice::from_ref(&shared))?;
        let mode = |path: &Path| std::fs::metadata(path).map(|found| found.permissions().mode() & 0o777);
        assert_eq!(mode(&secret)?, 0o640);
        assert_eq!(mode(data.path())?, 0o711);
        assert_eq!(mode(&shared)?, 0o755);
        Ok(())
    }

    #[test]
    fn the_rules_come_from_the_config() {
        let rules = configured(Path::new("/data"));
        assert_eq!(rules.homes, Path::new("/data/agent-homes"));
        assert!(rules.uids > 0 && rules.first_uid > 0);
        assert!(rules.kept.iter().any(|name| name == "PATH"));
        assert!(!rules.kept.iter().any(|name| name.starts_with("AWS")));
    }
}
