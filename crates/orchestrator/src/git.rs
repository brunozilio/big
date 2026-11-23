use std::path::{Path, PathBuf};

fn run(dir: &Path, args: &[&str]) -> Option<String> {
    let output = platform::git::command(dir).args(args).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn is_repo(dir: &Path) -> bool {
    dir.is_dir() && run(dir, &["rev-parse", "--is-inside-work-tree"]).is_some_and(|answer| answer == "true")
}

pub fn head(dir: &Path) -> Option<String> {
    is_repo(dir).then(|| run(dir, &["rev-parse", "HEAD"])).flatten()
}

pub fn branch_name(item: &str) -> String {
    format!("{}/{item}", brand::COMMAND)
}

pub fn folder_for(branch: &str) -> String {
    branch.chars().map(|letter| if letter.is_ascii_alphanumeric() || letter == '-' { letter } else { '_' }).collect()
}

pub fn origin(dir: &Path) -> Option<String> {
    is_repo(dir).then(|| run(dir, &["remote", "get-url", "origin"])).flatten().filter(|url| !url.is_empty())
}

fn attempt(dir: &Path, args: &[&str]) -> std::io::Result<String> {
    let output = platform::git::command(dir).args(args).output()?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned());
    }
    let said = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(std::io::Error::other(format!("git {}: {said}", args.join(" "))))
}

fn started(repo: &Path) -> std::io::Result<()> {
    if run(repo, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_some() {
        return Ok(());
    }
    attempt(repo, &["-c", "commit.gpgsign=false", "commit", "--allow-empty", "-m", "Initial commit"])?;
    if origin(repo).is_some() {
        attempt(repo, &["push", "-u", "origin", "HEAD"])?;
    }
    Ok(())
}

pub fn worktree(repo: Option<&Path>, data: &Path, branch: &str, folder: &str) -> std::io::Result<PathBuf> {
    let path = data.join("worktrees").join(folder);
    if path.exists() {
        return Ok(path);
    }
    std::fs::create_dir_all(data.join("worktrees"))?;
    let Some(repo) = repo.filter(|repo| is_repo(repo)) else {
        std::fs::create_dir_all(&path)?;
        return Ok(path);
    };
    started(repo)?;
    let place = path.to_string_lossy().into_owned();
    let known = run(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_some();
    if known {
        attempt(repo, &["worktree", "add", &place, branch])?;
    } else {
        attempt(repo, &["worktree", "add", "-b", branch, &place, "HEAD"])?;
    }
    Ok(path)
}

pub fn restore(dir: &Path, commit: &str) -> bool {
    is_repo(dir) && run(dir, &["reset", "--hard", commit]).is_some() && run(dir, &["clean", "-fd"]).is_some()
}

pub fn push(dir: &Path, branch: &str) -> bool {
    let remote = run(dir, &["remote"]).unwrap_or_default();
    if !remote.lines().any(|name| name == "origin") {
        return false;
    }
    run(dir, &["push", "-u", "origin", branch]).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) -> String {
        run(dir, args).unwrap_or_else(|| panic!("git {args:?}"))
    }

    #[test]
    fn an_empty_repository_gets_a_first_commit_and_a_real_worktree() {
        let repo = tempfile::tempdir().expect("repo");
        let data = tempfile::tempdir().expect("data");
        git(repo.path(), &["init", "-q", "-b", "main"]);
        git(repo.path(), &["config", "user.name", "T"]);
        git(repo.path(), &["config", "user.email", "t@example.com"]);

        let made = worktree(Some(repo.path()), data.path(), "big/x", "big_x").expect("worktree");

        assert!(is_repo(&made), "the worktree is a git checkout");
        assert_eq!(git(&made, &["branch", "--show-current"]), "big/x");
        assert_eq!(git(repo.path(), &["log", "--format=%s", "main"]), "Initial commit");
    }

    #[test]
    fn a_worktree_that_git_refuses_is_an_error_not_a_plain_folder() {
        let repo = tempfile::tempdir().expect("repo");
        let data = tempfile::tempdir().expect("data");
        git(repo.path(), &["init", "-q", "-b", "main"]);
        git(repo.path(), &["-c", "user.name=T", "-c", "user.email=t@example.com", "commit", "-q", "--allow-empty", "-m", "c"]);

        let refused = worktree(Some(repo.path()), data.path(), "main", "taken");

        assert!(refused.is_err(), "main is already checked out");
        assert!(!data.path().join("worktrees/taken").exists());
    }
}
