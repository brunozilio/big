use std::path::{Path, PathBuf};

use domain::workflow::WorkItem;

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
    format!("{}/{item}", brand::workflow::BRANCH_PREFIX)
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

pub fn head_of(item: &WorkItem, repo: &str) -> String {
    wanted(item, repo, "head").or_else(|| item.branch.clone()).unwrap_or_else(|| branch_name(item.id.as_str()))
}

pub fn base_of(item: &WorkItem, repo: &str) -> Option<String> {
    wanted(item, repo, "base")
}

fn wanted(item: &WorkItem, repo: &str, name: &str) -> Option<String> {
    let text = item.input["git"][repo][name].as_str().map(str::trim)?;
    (!text.is_empty()).then(|| text.to_owned())
}

fn exists(repo: &Path, reference: &str) -> bool {
    run(repo, &["rev-parse", "--verify", "--quiet", &format!("{reference}^{{commit}}")]).is_some()
}

fn default_branch(repo: &Path) -> Option<String> {
    let known = || run(repo, &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"]);
    let found = known().or_else(|| run(repo, &["remote", "set-head", "origin", "--auto"]).and_then(|_| known()))?;
    found.strip_prefix("refs/remotes/").map(str::to_owned)
}

fn start_point(repo: &Path, base: Option<&str>, remote: bool) -> std::io::Result<String> {
    let Some(base) = base else {
        return Ok(remote.then(|| default_branch(repo)).flatten().unwrap_or_else(|| "HEAD".to_owned()));
    };
    let tracked = format!("refs/remotes/origin/{base}");
    if remote && exists(repo, &tracked) {
        return Ok(tracked);
    }
    let local = format!("refs/heads/{base}");
    if exists(repo, &local) {
        return Ok(local);
    }
    Err(std::io::Error::other(format!("the base branch {base} does not exist")))
}

pub fn worktree(
    repo: Option<&Path>,
    data: &Path,
    branch: &str,
    base: Option<&str>,
    folder: &str,
) -> std::io::Result<PathBuf> {
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
    let remote = origin(repo).is_some();
    if remote {
        run(repo, &["fetch", "--quiet", "origin"]);
    }
    let place = path.to_string_lossy().into_owned();
    let remote_head = format!("refs/remotes/origin/{branch}");
    if exists(repo, &format!("refs/heads/{branch}")) {
        attempt(repo, &["worktree", "add", &place, branch])?;
    } else if remote && exists(repo, &remote_head) {
        attempt(repo, &["worktree", "add", "--track", "-b", branch, &place, &remote_head])?;
    } else {
        let start = start_point(repo, base, remote)?;
        attempt(repo, &["worktree", "add", "--no-track", "-b", branch, &place, &start])?;
    }
    Ok(path)
}

pub fn detached(repo: &Path, data: &Path, folder: &str) -> std::io::Result<Option<PathBuf>> {
    let Some(commit) = head(repo) else { return Ok(None) };
    let path = data.join("worktrees").join(folder);
    if path.exists() {
        attempt(&path, &["checkout", "--quiet", "--detach", "--force", &commit])?;
        return Ok(Some(path));
    }
    std::fs::create_dir_all(data.join("worktrees"))?;
    let place = path.to_string_lossy().into_owned();
    attempt(repo, &["worktree", "add", "--quiet", "--detach", &place, &commit])?;
    Ok(Some(path))
}

pub fn remove_worktree(repo: &Path, path: &Path) -> std::io::Result<()> {
    let place = path.to_string_lossy().into_owned();
    attempt(repo, &["worktree", "remove", &place]).map(|_| ())
}

pub fn prune_worktrees(repo: &Path) {
    run(repo, &["worktree", "prune"]);
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

        let made = worktree(Some(repo.path()), data.path(), "big/x", None, "big_x").expect("worktree");

        assert!(is_repo(&made), "the worktree is a git checkout");
        assert_eq!(git(&made, &["branch", "--show-current"]), "big/x");
        assert_eq!(git(repo.path(), &["log", "--format=%s", "main"]), "Initial commit");
    }

    #[test]
    fn a_clean_worktree_is_removed_and_its_branch_kept_but_a_dirty_one_stays() {
        let repo = tempfile::tempdir().expect("repo");
        let data = tempfile::tempdir().expect("data");
        git(repo.path(), &["init", "-q", "-b", "main"]);
        git(repo.path(), &["config", "user.name", "T"]);
        git(repo.path(), &["config", "user.email", "t@example.com"]);
        let clean = worktree(Some(repo.path()), data.path(), "big/a", None, "big_a").expect("clean");
        let dirty = worktree(Some(repo.path()), data.path(), "big/b", None, "big_b").expect("dirty");
        std::fs::write(dirty.join("draft.txt"), "wip").expect("draft");

        remove_worktree(repo.path(), &clean).expect("removed");
        let kept = remove_worktree(repo.path(), &dirty);

        assert!(!clean.exists());
        assert!(exists(repo.path(), "refs/heads/big/a"), "the branch outlives its worktree");
        assert!(kept.is_err() && dirty.join("draft.txt").exists(), "unsaved work is never thrown away");
        let again = worktree(Some(repo.path()), data.path(), "big/a", None, "big_a").expect("again");
        assert_eq!(git(&again, &["branch", "--show-current"]), "big/a");
    }

    #[test]
    fn a_worktree_that_git_refuses_is_an_error_not_a_plain_folder() {
        let repo = tempfile::tempdir().expect("repo");
        let data = tempfile::tempdir().expect("data");
        git(repo.path(), &["init", "-q", "-b", "main"]);
        git(
            repo.path(),
            &["-c", "user.name=T", "-c", "user.email=t@example.com", "commit", "-q", "--allow-empty", "-m", "c"],
        );

        let refused = worktree(Some(repo.path()), data.path(), "main", None, "taken");

        assert!(refused.is_err(), "main is already checked out");
        assert!(!data.path().join("worktrees/taken").exists());
    }

    fn commit(dir: &Path, message: &str) -> String {
        git(
            dir,
            &["-c", "user.name=T", "-c", "user.email=t@example.com", "commit", "-q", "--allow-empty", "-m", message],
        );
        git(dir, &["rev-parse", "HEAD"])
    }

    struct Remote {
        root: tempfile::TempDir,
    }

    impl Remote {
        fn new() -> Remote {
            let root = tempfile::tempdir().expect("root");
            let bare = root.path().join("remote.git");
            let seed = root.path().join("seed");
            git(root.path(), &["init", "-q", "--bare", "-b", "trunk", &bare.to_string_lossy()]);
            git(root.path(), &["clone", "-q", &bare.to_string_lossy(), &seed.to_string_lossy()]);
            git(&seed, &["checkout", "-q", "-b", "trunk"]);
            commit(&seed, "first");
            git(&seed, &["push", "-q", "-u", "origin", "trunk"]);
            git(root.path(), &["clone", "-q", &bare.to_string_lossy(), &root.path().join("local").to_string_lossy()]);
            Remote { root }
        }

        fn seed(&self) -> PathBuf {
            self.root.path().join("seed")
        }

        fn local(&self) -> PathBuf {
            self.root.path().join("local")
        }

        fn pushed(&self, branch: &str, message: &str) -> String {
            let seed = self.seed();
            git(&seed, &["checkout", "-q", "-B", branch]);
            let made = commit(&seed, message);
            git(&seed, &["push", "-q", "origin", branch]);
            made
        }
    }

    #[test]
    fn a_new_branch_starts_from_the_remote_default_branch_not_the_local_checkout() {
        let remote = Remote::new();
        let data = tempfile::tempdir().expect("data");
        let local = remote.local();
        git(&local, &["checkout", "-q", "-b", "elsewhere"]);
        let stray = commit(&local, "local only");
        git(&local, &["remote", "set-head", "origin", "-d"]);
        let tip = remote.pushed("trunk", "second");

        let made = worktree(Some(&local), data.path(), "big/x", None, "big_x").expect("worktree");

        assert_eq!(git(&made, &["branch", "--show-current"]), "big/x");
        assert_eq!(git(&made, &["rev-parse", "HEAD"]), tip);
        assert_ne!(tip, stray);
    }

    #[test]
    fn an_explicit_base_is_taken_from_the_remote() {
        let remote = Remote::new();
        let data = tempfile::tempdir().expect("data");
        let tip = remote.pushed("release", "release work");

        let made = worktree(Some(&remote.local()), data.path(), "big/y", Some("release"), "big_y").expect("worktree");

        assert_eq!(git(&made, &["rev-parse", "HEAD"]), tip);
        assert!(worktree(Some(&remote.local()), data.path(), "big/z", Some("missing"), "big_z").is_err());
    }

    #[test]
    fn an_existing_head_branch_on_the_remote_is_checked_out_and_tracked() {
        let remote = Remote::new();
        let data = tempfile::tempdir().expect("data");
        let tip = remote.pushed("feature/pr", "pull request work");

        let made = worktree(Some(&remote.local()), data.path(), "feature/pr", Some("trunk"), "pr").expect("worktree");

        assert_eq!(git(&made, &["branch", "--show-current"]), "feature/pr");
        assert_eq!(git(&made, &["rev-parse", "HEAD"]), tip);
        assert_eq!(git(&made, &["rev-parse", "--abbrev-ref", "@{upstream}"]), "origin/feature/pr");
    }

    #[test]
    fn an_existing_local_head_branch_is_reused() {
        let remote = Remote::new();
        let data = tempfile::tempdir().expect("data");
        let local = remote.local();
        git(&local, &["checkout", "-q", "-b", "mine"]);
        let tip = commit(&local, "mine");
        git(&local, &["checkout", "-q", "trunk"]);

        let made = worktree(Some(&local), data.path(), "mine", None, "mine").expect("worktree");

        assert_eq!(git(&made, &["rev-parse", "HEAD"]), tip);
    }

    #[test]
    fn the_head_and_base_come_from_the_item_or_the_generated_branch() {
        let mut item = WorkItem { id: "i1".into(), input: serde_json::json!({}), ..WorkItem::default() };
        assert_eq!(head_of(&item, "r1"), branch_name("i1"));
        assert_eq!(base_of(&item, "r1"), None);
        item.input =
            serde_json::json!({ "git": { "r1": { "base": "develop", "head": "feature/x" }, "r2": { "base": "" } } });
        assert_eq!(head_of(&item, "r1"), "feature/x");
        assert_eq!(base_of(&item, "r1").as_deref(), Some("develop"));
        assert_eq!(head_of(&item, "r2"), branch_name("i1"));
        assert_eq!(base_of(&item, "r2"), None);
    }
}
