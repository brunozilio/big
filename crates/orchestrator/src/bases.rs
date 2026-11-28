use std::path::{Path, PathBuf};
use std::sync::Arc;

use domain::workflow::{ItemId, WorkflowEvent};
use serde::Serialize;
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::events::Wake;
use crate::git;
use crate::shared::Shared;

const NEEDS_BASE: &str = "the base branch is missing: ";

pub fn blocked(repository: &str, base: &str) -> String {
    format!("{NEEDS_BASE}{}", json!({ "repository": repository, "base": base }))
}

pub fn missing(reason: &str) -> Option<(String, String)> {
    let found: Value = serde_json::from_str(reason.strip_prefix(NEEDS_BASE)?).ok()?;
    let (repository, base) = (found["repository"].as_str()?, found["base"].as_str()?);
    Some((repository.to_owned(), base.to_owned()))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BaseBranches {
    pub branches: Vec<String>,
    pub default: Option<String>,
}

fn own(name: &str) -> bool {
    name.starts_with(&format!("{}/", brand::workflow::BRANCH_PREFIX))
}

fn listed(repo: &Path) -> BaseBranches {
    let refs = ["for-each-ref", "--format=%(refname)", "refs/heads", "refs/remotes/origin"];
    let found = git::run(repo, &refs).unwrap_or_default();
    let mut branches: Vec<String> = found
        .lines()
        .filter_map(|line| line.strip_prefix("refs/heads/").or_else(|| line.strip_prefix("refs/remotes/origin/")))
        .filter(|name| *name != "HEAD" && !own(name))
        .map(str::to_owned)
        .collect();
    branches.sort();
    branches.dedup();
    let remote = git::default_branch(repo).and_then(|name| name.strip_prefix("origin/").map(str::to_owned));
    let current = || git::run(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"]).filter(|name| !own(name));
    let default = remote.or_else(current).filter(|name| branches.contains(name));
    BaseBranches { branches, default }
}

fn start_of(repo: &Path, from: &str, remote: bool) -> std::io::Result<String> {
    let tracked = format!("refs/remotes/origin/{from}");
    if remote && git::exists(repo, &tracked) {
        return Ok(tracked);
    }
    let local = format!("refs/heads/{from}");
    if git::exists(repo, &local) {
        return Ok(local);
    }
    Err(std::io::Error::other(git::MissingBase(from.to_owned())))
}

fn created(repo: &Path, base: &str, from: &str) -> std::io::Result<()> {
    git::attempt(repo, &["check-ref-format", "--branch", base])?;
    let remote = git::origin(repo).is_some();
    if remote {
        git::run(repo, &["fetch", "--quiet", "origin"]);
    }
    if !git::exists(repo, &format!("refs/heads/{base}")) {
        let start = start_of(repo, from, remote)?;
        git::attempt(repo, &["branch", "--no-track", base, &start])?;
    }
    if remote && !git::exists(repo, &format!("refs/remotes/origin/{base}")) {
        git::attempt(repo, &["push", "--quiet", "origin", &format!("refs/heads/{base}:refs/heads/{base}")])?;
    }
    Ok(())
}

fn folder_of(shared: &Shared, id: &ItemId, repository: &str) -> Result<PathBuf> {
    let item = shared.item(id)?;
    let checkout = shared.checkouts(&item).into_iter().find(|checkout| checkout.id == repository);
    let checkout = checkout.ok_or_else(|| Error::Branch(format!("the item has no repository {repository}")))?;
    checkout
        .directory
        .filter(|folder| git::is_repo(folder))
        .ok_or_else(|| Error::Branch(format!("the repository {} has no folder on this machine", checkout.name)))
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(work).await.map_err(|error| Error::Branch(error.to_string()))
}

pub async fn branches(shared: &Shared, id: &ItemId, repository: &str) -> Result<BaseBranches> {
    let folder = folder_of(shared, id, repository)?;
    blocking(move || listed(&folder)).await
}

pub enum Choice {
    Existing,
    CreatedFrom(String),
}

pub async fn choose(shared: &Arc<Shared>, id: &ItemId, repository: &str, base: &str, choice: Choice) -> Result<()> {
    let base = base.trim().to_owned();
    if base.is_empty() {
        return Err(Error::Branch("the base branch is empty".into()));
    }
    let folder = folder_of(shared, id, repository)?;
    let named = base.clone();
    match choice {
        Choice::CreatedFrom(from) => {
            let made = blocking(move || created(&folder, &named, &from)).await?;
            made.map_err(|error| Error::Branch(format!("the branch {base} could not be created: {error}")))?;
        }
        Choice::Existing => {
            let known = blocking(move || listed(&folder)).await?;
            if !known.branches.contains(&base) {
                return Err(Error::Branch(format!("the branch {base} does not exist")));
            }
        }
    }
    shared.append(id, &[git::chosen_base(repository, base)])?;
    let item = shared.item(id)?;
    if item.impediment.is_some() {
        shared.append(id, &[WorkflowEvent::ImpedimentSet { on: false, reason: None }])?;
    }
    crate::runner::ensure(shared, id);
    shared.wake(id, Wake::Rerun);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let done = platform::git::command(dir).args(args).output().expect("git");
        assert!(done.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&done.stderr));
    }

    fn commit(dir: &Path) {
        git(dir, &["-c", "user.name=T", "-c", "user.email=t@example.com", "commit", "-q", "--allow-empty", "-m", "c"]);
    }

    #[test]
    fn the_reason_carries_the_repository_and_the_missing_branch() {
        let reason = blocked("r1", "release/2");
        assert_eq!(missing(&reason), Some(("r1".into(), "release/2".into())));
        assert_eq!(missing("the base branch develop does not exist"), None);
    }

    #[test]
    fn a_missing_base_is_created_from_another_branch_and_pushed() {
        let root = tempfile::tempdir().expect("root");
        let (bare, local) = (root.path().join("remote.git"), root.path().join("local"));
        git(root.path(), &["init", "-q", "--bare", "-b", "main", &bare.to_string_lossy()]);
        git(root.path(), &["clone", "-q", &bare.to_string_lossy(), &local.to_string_lossy()]);
        git(&local, &["checkout", "-q", "-b", "main"]);
        commit(&local);
        git(&local, &["push", "-q", "-u", "origin", "main"]);
        git(&local, &["branch", &format!("{}/x", brand::workflow::BRANCH_PREFIX)]);

        let before = listed(&local);
        assert_eq!(before.branches, vec!["main".to_owned()], "the product's own branches are not offered");
        assert_eq!(before.default.as_deref(), Some("main"));

        created(&local, "develop", "main").expect("created");
        assert!(git::exists(&bare, "refs/heads/develop"), "the new base reaches the remote");
        assert_eq!(listed(&local).branches, vec!["develop".to_owned(), "main".to_owned()]);
        assert!(created(&local, "-x", "main").is_err(), "a name git refuses stays refused");
        assert!(created(&local, "release", "nope").is_err());
    }
}
