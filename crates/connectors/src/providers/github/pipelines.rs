use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::Api;
use super::refs::{repo_of, run_of, run_ref};
use super::states::state;
use super::workflow::{WorkflowJob, jobs, matches};
use crate::error::{ActionError, ErrorClass};
use crate::http::Method;
use crate::provider::{Args, text_of};

const DEFAULT_TAIL: u64 = 500;
const DEFAULT_RUNS: u64 = 20;

fn list(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn finished_at(run: &Value, state: &str) -> Value {
    if matches!(state, "passed" | "failed" | "cancelled" | "skipped") { run["updated_at"].clone() } else { Value::Null }
}

pub fn run_summary(repo: &str, run: &Value) -> Value {
    let id = run["id"].as_u64().unwrap_or_default();
    let state = state(&run["status"], &run["conclusion"]);
    json!({
        "ref": run_ref(repo, id),
        "repo": repo,
        "pipeline": text_of(run, "/name"),
        "file": text_of(run, "/path"),
        "event": text_of(run, "/event"),
        "actor": text_of(run, "/triggering_actor/login"),
        "head": text_of(run, "/head_branch"),
        "sha": text_of(run, "/head_sha"),
        "state": state,
        "attempt": run["run_attempt"].as_u64().unwrap_or(1),
        "url": text_of(run, "/html_url"),
        "started": run["run_started_at"].as_str().or(run["created_at"].as_str()),
        "finished": finished_at(run, state),
    })
}

fn sha_like(text: &str) -> bool {
    text.len() == 40 && text.chars().all(|c| c.is_ascii_hexdigit())
}

fn runs_path(repo: &str, args: Args<'_>) -> Result<String, ActionError> {
    let limit = args.optional_number("limit").unwrap_or(DEFAULT_RUNS).clamp(1, 100);
    let base = match args.optional_text("pipeline").or_else(|| args.optional_text("workflow")) {
        Some(file) if file.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) => {
            format!("/repos/{repo}/actions/workflows/{file}/runs")
        }
        Some(file) => return Err(ActionError::permanent(format!("{file:?} is not a pipeline file or id"))),
        None => format!("/repos/{repo}/actions/runs"),
    };
    let mut url =
        url::Url::parse(&format!("http://x{base}")).map_err(|error| ActionError::permanent(error.to_string()))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("per_page", &limit.to_string());
        if let Some(reference) = args.optional_text("ref") {
            query.append_pair(if sha_like(reference) { "head_sha" } else { "branch" }, reference);
        }
    }
    Ok(format!("{}?{}", url.path(), url.query().unwrap_or_default()))
}

pub async fn runs(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let repo = repo_of(api, args)?;
    let answer = api.get(&runs_path(&repo, args)?).await?;
    Ok(Value::Array(list(&answer["workflow_runs"]).iter().map(|run| run_summary(&repo, run)).collect()))
}

async fn workflow_jobs(api: &Api<'_>, repo: &str, run: &Value) -> Result<Vec<WorkflowJob>, ActionError> {
    let path = text_of(run, "/path");
    let path = path.split('@').next().unwrap_or_default();
    if path.is_empty() {
        return Ok(Vec::new());
    }
    let sha = text_of(run, "/head_sha");
    match api.get(&format!("/repos/{repo}/contents/{path}?ref={sha}")).await {
        Ok(file) => {
            let encoded: String = text_of(&file, "/content").chars().filter(|c| !c.is_whitespace()).collect();
            let bytes = STANDARD.decode(encoded).unwrap_or_default();
            Ok(jobs(&String::from_utf8_lossy(&bytes)))
        }
        Err(error) if error.class == ErrorClass::Permanent => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

fn step_view(step: &Value) -> Value {
    json!({
        "name": text_of(step, "/name"),
        "state": state(&step["status"], &step["conclusion"]),
        "started": step["started_at"],
        "finished": step["completed_at"],
    })
}

fn job_view(job: &Value, graph: &[WorkflowJob]) -> Value {
    let name = text_of(job, "/name");
    let known = graph.iter().find(|known| matches(known, &name));
    json!({
        "id": job["id"].to_string(),
        "name": name,
        "key": known.map(|known| known.key.clone()),
        "needs": known.map(|known| known.needs.clone()).unwrap_or_default(),
        "state": state(&job["status"], &job["conclusion"]),
        "started": job["started_at"],
        "finished": job["completed_at"],
        "url": job["html_url"],
        "steps": list(&job["steps"]).iter().map(step_view).collect::<Vec<_>>(),
    })
}

pub async fn run(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let (repo, id) = run_of(api, args)?;
    let run = api.get(&format!("/repos/{repo}/actions/runs/{id}")).await?;
    let listed = api.get(&format!("/repos/{repo}/actions/runs/{id}/jobs?per_page=100&filter=latest")).await?;
    let graph = workflow_jobs(api, &repo, &run).await?;
    let jobs: Vec<Value> = list(&listed["jobs"]).iter().map(|job| job_view(job, &graph)).collect();
    let mut summary = run_summary(&repo, &run);
    summary["jobs"] = json!(jobs);
    Ok(summary)
}

pub async fn job_log(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let (repo, _) = run_of(api, args)?;
    let job = args.number("job")?;
    let tail = usize::try_from(args.optional_number("tail").unwrap_or(DEFAULT_TAIL)).unwrap_or(usize::MAX);
    let log = api.raw(api.request(Method::Get, &format!("/repos/{repo}/actions/jobs/{job}/logs"))).await?.text();
    let lines: Vec<&str> = log.lines().collect();
    let kept = lines[lines.len().saturating_sub(tail)..].join("\n");
    Ok(json!({ "text": kept, "truncated": lines.len() > tail }))
}

fn deployment_view(deployment: &Value, reference: &str) -> Value {
    let reviewers: Vec<String> = list(&deployment["reviewers"])
        .iter()
        .map(|reviewer| text_of(reviewer, "/reviewer/login"))
        .filter(|login| !login.is_empty())
        .collect();
    json!({
        "id": deployment["environment"]["id"].to_string(),
        "environment": deployment["environment"]["name"],
        "run": reference,
        "reviewers": reviewers,
        "canApprove": deployment["current_user_can_approve"].as_bool().unwrap_or(false),
    })
}

async fn pending(api: &Api<'_>, repo: &str, run: u64) -> Result<Vec<Value>, ActionError> {
    let answer = api.get(&format!("/repos/{repo}/actions/runs/{run}/pending_deployments")).await?;
    let reference = run_ref(repo, run);
    Ok(list(&answer).iter().map(|deployment| deployment_view(deployment, &reference)).collect())
}

pub async fn pending_deployments(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let (repo, run) = run_of(api, args)?;
    Ok(Value::Array(pending(api, &repo, run).await?))
}

fn chosen(deployment: &Value, wanted: &[Value]) -> bool {
    wanted.is_empty()
        || wanted.iter().any(|want| {
            let text = want.as_str().map_or_else(|| want.to_string(), str::to_owned);
            deployment["id"] == text.as_str() || deployment["environment"] == text.as_str()
        })
}

pub async fn review_deployment(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let (repo, run) = run_of(api, args)?;
    let decision = args.text("decision")?;
    let state = match decision {
        "approve" => "approved",
        "reject" => "rejected",
        other => return Err(ActionError::permanent(format!("{other:?} is not approve or reject"))),
    };
    let wanted = [args.list("deployments"), args.list("environments")].concat();
    let waiting: Vec<Value> =
        pending(api, &repo, run).await?.into_iter().filter(|deployment| chosen(deployment, &wanted)).collect();
    if waiting.is_empty() {
        return Ok(json!({ "decision": decision, "deployments": [], "already": true }));
    }
    if waiting.iter().any(|deployment| deployment["canApprove"] == false) {
        return Err(ActionError::permanent("this account cannot review the deployment"));
    }
    let ids: Vec<u64> = waiting.iter().filter_map(|deployment| deployment["id"].as_str()?.parse().ok()).collect();
    let body =
        json!({ "environment_ids": ids, "state": state, "comment": args.optional_text("comment").unwrap_or_default() });
    api.send(Method::Post, &format!("/repos/{repo}/actions/runs/{run}/pending_deployments"), &body).await?;
    let reviewed: Vec<&Value> = waiting.iter().map(|deployment| &deployment["id"]).collect();
    Ok(json!({ "decision": decision, "deployments": reviewed, "already": false }))
}
