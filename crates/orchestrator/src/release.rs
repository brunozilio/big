use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use domain::workflow::WorkItem;

use crate::git;
use crate::repos::{self, Checkout};
use crate::shared::Shared;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Place {
    pub repo: PathBuf,
    pub path: PathBuf,
}

pub fn release(shared: &Arc<Shared>, item: &WorkItem) {
    let mut places = places(&shared.config.data, item, &shared.checkouts(item));
    if places.is_empty() {
        return;
    }
    let others = match shared.store.active_items() {
        Ok(others) => others,
        Err(error) => {
            tracing::warn!(%error, item = %item.id, "the item's worktrees were kept: the active items are unknown");
            return;
        }
    };
    for other in others.iter().filter(|other| other.id != item.id) {
        for used in places_of(&shared.config.data, other, &shared.checkouts(other)) {
            places.remove(&used);
        }
    }
    let id = item.id.clone();
    shared.spawn(async move {
        let removed = tokio::task::spawn_blocking(move || remove(&places)).await;
        for error in removed.into_iter().flatten() {
            tracing::warn!(%error, item = %id, "a worktree was kept");
        }
    });
}

fn branches(item: &WorkItem, repo: &str) -> BTreeSet<String> {
    [Some(git::head_of(item, repo)), item.branch.clone(), Some(git::branch_name(item.id.as_str()))]
        .into_iter()
        .flatten()
        .collect()
}

fn places_of(data: &Path, item: &WorkItem, checkouts: &[Checkout]) -> BTreeSet<Place> {
    let mut found = BTreeSet::new();
    for checkout in checkouts {
        let Some(repo) = &checkout.directory else { continue };
        for branch in branches(item, &checkout.id) {
            let path = data.join("worktrees").join(repos::folder(&branch, &checkout.id));
            found.insert(Place { repo: repo.clone(), path });
        }
    }
    found
}

pub fn places(data: &Path, item: &WorkItem, checkouts: &[Checkout]) -> BTreeSet<Place> {
    places_of(data, item, checkouts).into_iter().filter(|place| place.path.exists()).collect()
}

pub fn remove(places: &BTreeSet<Place>) -> Vec<String> {
    let mut errors = Vec::new();
    let mut repos = BTreeSet::new();
    for place in places {
        if let Err(error) = git::remove_worktree(&place.repo, &place.path) {
            errors.push(error.to_string());
        }
        repos.insert(place.repo.clone());
    }
    for repo in repos {
        git::prune_worktrees(&repo);
    }
    errors
}
