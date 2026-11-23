use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use connectors::{ActionError, Http, HttpRequest, HttpResponse, Method};
use serde_json::{Value, json};

pub const API: &str = "https://code.test";
pub const REPO: &str = "acme/app";
pub const ENVIRONMENT: u64 = 7;

#[derive(Debug, Clone)]
pub struct Review {
    pub id: u64,
    pub state: String,
    pub body: String,
    pub lines: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct Pull {
    pub repo: String,
    pub number: u64,
    pub head: String,
    pub base: String,
    pub title: String,
    pub body: String,
    pub state: String,
    pub merged_sha: Option<String>,
    pub updated: String,
    pub reviews: Vec<Review>,
}

#[derive(Debug, Clone)]
pub struct Run {
    pub repo: String,
    pub id: u64,
    pub name: String,
    pub branch: String,
    pub sha: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub updated: String,
}

#[derive(Debug, Default)]
pub struct State {
    pub pulls: Vec<Pull>,
    pub runs: Vec<Run>,
    pub ci: VecDeque<String>,
    pub sent: Vec<String>,
    pub merges: usize,
    pub deploy_reviews: Vec<String>,
    pub ci_running: bool,
    next: u64,
}

impl State {
    fn next(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
}

#[derive(Debug)]
pub struct Github {
    repos: Mutex<Vec<(String, PathBuf)>>,
    state: Mutex<State>,
}

pub fn now() -> String {
    jiff::Timestamp::now().to_string()
}

fn reply(status: u16, body: &Value) -> Result<HttpResponse, ActionError> {
    Ok(HttpResponse::new(status, body.to_string()))
}

fn missing() -> Result<HttpResponse, ActionError> {
    reply(404, &json!({ "message": "Not Found" }))
}

fn decoded(text: &str) -> String {
    text.replace("%3A", ":").replace("%2F", "/")
}

fn query(path: &str, name: &str) -> Option<String> {
    let (_, query) = path.split_once('?')?;
    query.split('&').find_map(|pair| pair.strip_prefix(&format!("{name}="))).map(decoded)
}

fn pull_json(pull: &Pull) -> Value {
    json!({
        "number": pull.number, "title": pull.title, "body": pull.body, "state": pull.state,
        "merged": pull.merged_sha.is_some(), "merged_at": pull.merged_sha.as_ref().map(|_| pull.updated.clone()),
        "merge_commit_sha": pull.merged_sha, "draft": false, "mergeable": true,
        "html_url": format!("https://code.test/{}/pull/{}", pull.repo, pull.number),
        "head": { "ref": pull.head, "sha": format!("head-{}", pull.number) }, "base": { "ref": pull.base },
        "user": { "login": "agente" }, "updated_at": pull.updated, "closed_at": pull.updated,
    })
}

fn run_json(run: &Run) -> Value {
    json!({
        "id": run.id, "name": run.name, "path": "", "event": "push", "status": run.status,
        "conclusion": run.conclusion, "head_branch": run.branch, "head_sha": run.sha, "run_attempt": 1,
        "html_url": format!("https://code.test/{}/actions/runs/{}", run.repo, run.id), "created_at": run.updated,
        "updated_at": run.updated,
    })
}

impl Github {
    pub fn new(repo: PathBuf) -> Github {
        Github { repos: Mutex::new(vec![(REPO.to_owned(), repo)]), state: Mutex::default() }
    }

    pub fn add(&self, name: &str, dir: PathBuf) {
        self.repos.lock().unwrap_or_else(PoisonError::into_inner).push((name.to_owned(), dir));
    }

    fn dir_of(&self, name: &str) -> Option<PathBuf> {
        let repos = self.repos.lock().unwrap_or_else(PoisonError::into_inner);
        repos.iter().find(|(known, _)| known == name).map(|(_, dir)| dir.clone())
    }

    pub fn pulls_of(&self, name: &str) -> usize {
        self.state().pulls.iter().filter(|pull| pull.repo == name).count()
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn fail_next_ci(&self) {
        self.state().ci.push_back("failure".into());
    }

    pub fn pulls_created(&self) -> usize {
        self.state().sent.iter().filter(|line| line.as_str() == "POST pulls").count()
    }

    pub fn reviews(&self) -> Vec<Review> {
        self.state().pulls.iter().flat_map(|pull| pull.reviews.clone()).collect()
    }

    pub fn close_pull(&self, number: u64) {
        let mut state = self.state();
        if let Some(pull) = state.pulls.iter_mut().find(|pull| pull.number == number) {
            pull.state = "closed".into();
            pull.updated = now();
        }
    }

    pub fn finish_ci(&self, conclusion: &str) {
        let mut state = self.state();
        state.ci_running = false;
        for run in state.runs.iter_mut().filter(|run| run.status == "in_progress") {
            run.status = "completed".into();
            run.conclusion = Some(conclusion.into());
            run.updated = now();
        }
    }

    pub fn ci_runs(&self) -> usize {
        self.state().runs.iter().filter(|run| run.name == "CI").count()
    }

    pub fn approve_deploy(&self) {
        let mut state = self.state();
        for run in state.runs.iter_mut().filter(|run| run.status == "waiting") {
            run.status = "completed".into();
            run.conclusion = Some("success".into());
            run.updated = now();
        }
    }

    fn head_of(&self, repo: &str, branch: &str) -> Option<String> {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(self.dir_of(repo)?)
            .args(["rev-parse", &format!("refs/heads/{branch}")])
            .output()
            .ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    fn ensure_run(&self, state: &mut State, repo: &str, branch: &str) {
        let Some(sha) = self.head_of(repo, branch) else { return };
        if state.runs.iter().any(|run| run.sha == sha) {
            return;
        }
        let conclusion = state.ci.pop_front().unwrap_or_else(|| "success".into());
        let id = state.next();
        let running = state.ci_running;
        let run = Run {
            repo: repo.to_owned(),
            id,
            name: "CI".into(),
            branch: branch.to_owned(),
            sha,
            status: if running { "in_progress" } else { "completed" }.into(),
            conclusion: (!running).then_some(conclusion),
            updated: now(),
        };
        state.runs.push(run);
    }

    fn route(&self, method: Method, path: &str, body: &Value) -> Result<HttpResponse, ActionError> {
        let mut state = self.state();
        let Some((repo, rest)) = self.split(path) else {
            return if path == "/graphql" { reply(200, &threads(&state, body)) } else { missing() };
        };
        let rest = rest.as_str();
        let bare = rest.split('?').next().unwrap_or_default();
        let parts: Vec<&str> = bare.split('/').filter(|part| !part.is_empty()).collect();
        match (method, parts.as_slice()) {
            (Method::Get, []) => reply(200, &json!({ "default_branch": "main" })),
            (Method::Get, ["pulls"]) => {
                let wanted =
                    query(rest, "head").map(|head| head.split_once(':').map_or(head.clone(), |(_, b)| b.to_owned()));
                let listed: Vec<Value> = state
                    .pulls
                    .iter()
                    .filter(|pull| pull.repo == repo && wanted.as_ref().is_none_or(|head| &pull.head == head))
                    .map(pull_json)
                    .collect();
                reply(200, &json!(listed))
            }
            (Method::Post, ["pulls"]) => {
                state.sent.push("POST pulls".into());
                let number = state.next();
                let pull = Pull {
                    repo: repo.clone(),
                    number,
                    head: body["head"].as_str().unwrap_or_default().into(),
                    base: body["base"].as_str().unwrap_or_default().into(),
                    title: body["title"].as_str().unwrap_or_default().into(),
                    body: body["body"].as_str().unwrap_or_default().into(),
                    state: "open".into(),
                    merged_sha: None,
                    updated: now(),
                    reviews: Vec::new(),
                };
                let created = pull_json(&pull);
                state.pulls.push(pull);
                reply(201, &created)
            }
            (_, ["pulls", number, rest @ ..]) => pulls(&mut state, method, number, rest, body),
            (Method::Get, ["commits", _, "check-runs"]) => reply(200, &json!({ "check_runs": [] })),
            (Method::Get, ["actions", "runs"]) => {
                if let Some(branch) = query(rest, "branch") {
                    self.ensure_run(&mut state, &repo, &branch);
                }
                let branch = query(rest, "branch");
                let sha = query(rest, "head_sha");
                let listed: Vec<Value> = state
                    .runs
                    .iter()
                    .rev()
                    .filter(|run| run.repo == repo)
                    .filter(|run| branch.as_ref().is_none_or(|wanted| &run.branch == wanted))
                    .filter(|run| sha.as_ref().is_none_or(|wanted| &run.sha == wanted))
                    .map(run_json)
                    .collect();
                reply(200, &json!({ "workflow_runs": listed }))
            }
            (_, ["actions", "runs", id, rest @ ..]) => runs(&mut state, method, id, rest, body),
            (Method::Get, ["actions", "jobs", _, "logs"]) => Ok(HttpResponse::new(200, "erro: um teste falhou")),
            _ => missing(),
        }
    }
}

impl Github {
    fn split(&self, path: &str) -> Option<(String, String)> {
        let rest = path.strip_prefix("/repos/")?;
        let mut parts = rest.splitn(3, '/');
        let (owner, name) = (parts.next()?, parts.next()?);
        let name = name.split('?').next().unwrap_or_default();
        let repo = format!("{owner}/{name}");
        self.dir_of(&repo)?;
        let after = &rest[owner.len() + 1 + name.len()..];
        Some((repo, after.to_owned()))
    }
}

fn pulls(
    state: &mut State,
    method: Method,
    number: &str,
    rest: &[&str],
    body: &Value,
) -> Result<HttpResponse, ActionError> {
    let number: u64 = number.parse().unwrap_or_default();
    let next = state.next();
    let merges = &mut state.merges;
    let Some(pull) = state.pulls.iter_mut().find(|pull| pull.number == number) else { return missing() };
    match (method, rest) {
        (Method::Get, []) => reply(200, &pull_json(pull)),
        (Method::Get, ["reviews"]) => {
            let listed: Vec<Value> = pull
                .reviews
                .iter()
                .map(|review| json!({ "id": review.id, "state": review.state, "body": review.body, "user": { "login": "você" }, "submitted_at": now() }))
                .collect();
            reply(200, &json!(listed))
        }
        (Method::Post, ["reviews"]) => {
            let state = match body["event"].as_str() {
                Some("APPROVE") => "APPROVED",
                Some("REQUEST_CHANGES") => "CHANGES_REQUESTED",
                _ => "COMMENTED",
            };
            let lines = body["comments"].as_array().cloned().unwrap_or_default();
            let review =
                Review { id: next, state: state.into(), body: body["body"].as_str().unwrap_or_default().into(), lines };
            pull.reviews.push(review);
            pull.updated = now();
            reply(200, &json!({ "id": next, "state": state }))
        }
        (Method::Get, ["files"]) => reply(
            200,
            &json!([{ "filename": "work.txt", "status": "added", "additions": 1, "deletions": 0, "patch": "@@ -0,0 +1 @@\n+feito" }]),
        ),
        (Method::Put, ["merge"]) => {
            *merges += 1;
            let sha = format!("{next:040x}");
            pull.merged_sha = Some(sha.clone());
            pull.state = "closed".into();
            pull.updated = now();
            let run = Run {
                repo: pull.repo.clone(),
                id: next,
                name: "Deploy".into(),
                branch: pull.base.clone(),
                sha: sha.clone(),
                status: "waiting".into(),
                conclusion: None,
                updated: now(),
            };
            state.runs.push(run);
            reply(200, &json!({ "merged": true, "sha": sha }))
        }
        _ => missing(),
    }
}

fn runs(state: &mut State, method: Method, id: &str, rest: &[&str], body: &Value) -> Result<HttpResponse, ActionError> {
    let id: u64 = id.parse().unwrap_or_default();
    let reviews = &mut state.deploy_reviews;
    let Some(run) = state.runs.iter_mut().find(|run| run.id == id) else { return missing() };
    match (method, rest) {
        (Method::Get, []) => reply(200, &run_json(run)),
        (Method::Get, ["jobs"]) => {
            let job = json!({ "id": run.id * 10, "name": "test", "status": run.status, "conclusion": run.conclusion,
                "started_at": run.updated, "completed_at": run.updated, "steps": [] });
            reply(200, &json!({ "jobs": [job] }))
        }
        (Method::Get, ["pending_deployments"]) => {
            let waiting = run.status == "waiting";
            let listed = if waiting {
                json!([{ "environment": { "id": ENVIRONMENT, "name": "production" }, "current_user_can_approve": true, "reviewers": [] }])
            } else {
                json!([])
            };
            reply(200, &listed)
        }
        (Method::Post, ["pending_deployments"]) => {
            let approved = body["state"] == "approved";
            reviews.push(body["state"].as_str().unwrap_or_default().to_owned());
            run.status = "completed".into();
            run.conclusion = Some(if approved { "success" } else { "failure" }.into());
            run.updated = now();
            reply(200, &json!([]))
        }
        _ => missing(),
    }
}

fn threads(state: &State, body: &Value) -> Value {
    let number = body["variables"]["number"].as_u64().unwrap_or_default();
    let nodes: Vec<Value> = state
        .pulls
        .iter()
        .filter(|pull| pull.number == number)
        .flat_map(|pull| pull.reviews.iter())
        .flat_map(|review| review.lines.iter().map(move |line| (review.id, line)))
        .enumerate()
        .map(|(at, (review, line))| {
            json!({ "id": format!("T{review}-{at}"), "isResolved": false, "isOutdated": false, "path": line["path"], "line": line["line"],
                "comments": { "nodes": [{ "id": format!("C{review}-{at}"), "body": line["body"], "createdAt": now(), "author": { "login": "você" } }] } })
        })
        .collect();
    json!({ "data": { "repository": { "pullRequest": { "reviewThreads": { "nodes": nodes } } } } })
}

#[async_trait]
impl Http for Github {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ActionError> {
        let Some(path) = request.url.strip_prefix(API) else {
            return Err(ActionError::permanent(format!("nothing answers {}", request.url)));
        };
        let body = if request.body.is_empty() { Value::Null } else { request.body_json() };
        self.route(request.method, path, &body)
    }
}
