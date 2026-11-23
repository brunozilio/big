use serde_json::Value;

pub fn read(settings: &Value, owners: bool, dirs: &mut Vec<String>) -> Result<(), String> {
    let Some(list) = settings.get("permissions").and_then(|permissions| permissions.get("additionalDirectories"))
    else {
        return Ok(());
    };
    if !owners {
        return Ok(());
    }
    let list = list.as_array().ok_or("permissions.additionalDirectories must be a list of folders")?;
    for dir in list {
        let dir = dir.as_str().ok_or("permissions.additionalDirectories must be a list of folders")?;
        dirs.push(dir.to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::prompt::settings::load_trusted as load;

    #[test]
    fn only_the_owner_adds_folders() {
        let home = tempfile::tempdir().expect("home");
        let repo = tempfile::tempdir().expect("repo");
        let write = |path: std::path::PathBuf, text: &str| {
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(path, text).expect("write");
        };
        write(
            repo.path().join(concat!(brand::dir!(), "/settings.json")),
            r#"{"permissions": {"additionalDirectories": ["/"]}}"#,
        );
        write(
            repo.path().join(concat!(brand::dir!(), "/settings.local.json")),
            r#"{"permissions": {"additionalDirectories": ["../shared"]}}"#,
        );
        write(
            home.path().join(concat!(brand::dir!(), "/settings.json")),
            r#"{"permissions": {"additionalDirectories": ["~/notes"]}}"#,
        );
        let settings = load(repo.path(), Some(home.path())).expect("settings");
        assert_eq!(settings.added_dirs, ["~/notes", "../shared"], "the checked-in one is ignored");
    }
}
