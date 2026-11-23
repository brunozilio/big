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
    let place = path.to_string_lossy().into_owned();
    let known = run(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_some();
    let added = if known {
        run(repo, &["worktree", "add", &place, branch])
    } else {
        run(repo, &["worktree", "add", "-b", branch, &place, "HEAD"])
    };
    if added.is_none() {
        std::fs::create_dir_all(&path)?;
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
