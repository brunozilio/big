use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::workflow::{WorkflowJob, jobs, matches};
use super::{Api, repo};
use crate::error::{ActionError, ErrorClass};
use crate::http::Method;
use crate::provider::{Args, text_of};

const DEFAULT_TAIL: u64 = 500;
const DEFAULT_RUNS: u64 = 20;

pub fn run_summary(run: &Value) -> Value {
    json!({
        "id": run["id"],
        "name": run["name"],
        "workflow": run["path"],
        "event": run["event"],
        "status": run["status"],
        "conclusion": run["conclusion"],
        "ref": run["head_branch"],
        "sha": run["head_sha"],
        "attempt": run["run_attempt"],
        "url": run["html_url"],
        "at": run["created_at"],
        "updated": run["updated_at"],
    })
}

fn sha_like(text: &str) -> bool {
    text.len() == 40 && text.chars().all(|c| c.is_ascii_hexdigit())
}

pub async fn runs(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let repo = repo(args)?;
    let limit = args.optional_number("limit").unwrap_or(DEFAULT_RUNS).clamp(1, 100);
    let base = match args.optional_text("workflow") {
        Some(workflow) if workflow.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) => {
            format!("/repos/{repo}/actions/workflows/{workflow}/runs")
        }
        Some(workflow) => return Err(ActionError::permanent(format!("{workflow:?} is not a workflow file or id"))),
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
    let path = format!("{}?{}", url.path(), url.query().unwrap_or_default());
    let answer = api.get(&path).await?;
    let runs: Vec<Value> =
        answer["workflow_runs"].as_array().map(Vec::as_slice).unwrap_or_default().iter().map(run_summary).collect();
    Ok(json!({ "runs": runs }))
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

fn job_view(job: &Value, graph: &[WorkflowJob]) -> Value {
    let name = text_of(job, "/name");
    let known = graph.iter().find(|known| matches(known, &name));
    let steps: Vec<Value> = job["steps"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|step| json!({ "number": step["number"], "name": step["name"], "status": step["status"], "conclusion": step["conclusion"] }))
        .collect();
    json!({
        "id": job["id"],
        "name": name,
        "key": known.map(|known| known.key.clone()),
        "needs": known.map(|known| known.needs.clone()).unwrap_or_default(),
        "status": job["status"],
        "conclusion": job["conclusion"],
        "started": job["started_at"],
        "completed": job["completed_at"],
        "url": job["html_url"],
        "steps": steps,
    })
}

pub async fn run(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let repo = repo(args)?;
    let id = args.number("run")?;
    let run = api.get(&format!("/repos/{repo}/actions/runs/{id}")).await?;
    let listed = api.get(&format!("/repos/{repo}/actions/runs/{id}/jobs?per_page=100&filter=latest")).await?;
    let graph = workflow_jobs(api, repo, &run).await?;
    let jobs: Vec<Value> = listed["jobs"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|job| job_view(job, &graph))
        .collect();
    let mut summary = run_summary(&run);
    summary["jobs"] = json!(jobs);
    Ok(summary)
}

pub async fn job_log(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let repo = repo(args)?;
    let job = args.number("job")?;
    let tail = usize::try_from(args.optional_number("tail").unwrap_or(DEFAULT_TAIL)).unwrap_or(usize::MAX);
    let log = api.raw(api.request(Method::Get, &format!("/repos/{repo}/actions/jobs/{job}/logs"))).await?.text();
    let lines: Vec<&str> = log.lines().collect();
    let kept = lines[lines.len().saturating_sub(tail)..].join("\n");
    Ok(json!({ "log": kept, "truncated": lines.len() > tail }))
}

async fn pending(api: &Api<'_>, repo: &str, run: u64) -> Result<Vec<Value>, ActionError> {
    let answer = api.get(&format!("/repos/{repo}/actions/runs/{run}/pending_deployments")).await?;
    Ok(answer
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|deployment| {
            json!({
                "environment_id": deployment["environment"]["id"],
                "environment": deployment["environment"]["name"],
                "can_approve": deployment["current_user_can_approve"],
                "wait_timer": deployment["wait_timer"],
            })
        })
        .collect())
}

pub async fn pending_deployments(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let repo = repo(args)?;
    Ok(json!({ "deployments": pending(api, repo, args.number("run")?).await? }))
}

fn chosen(deployment: &Value, wanted: &[Value]) -> bool {
    wanted.is_empty()
        || wanted.iter().any(|want| {
            want == &deployment["environment_id"]
                || want == &deployment["environment"]
                || want
                    .as_str()
                    .and_then(|text| text.parse::<u64>().ok())
                    .is_some_and(|id| deployment["environment_id"].as_u64() == Some(id))
        })
}

pub async fn review_deployment(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let repo = repo(args)?;
    let run = args.number("run")?;
    let state = match args.text("decision")? {
        "approve" => "approved",
        "reject" => "rejected",
        other => return Err(ActionError::permanent(format!("{other:?} is not approve or reject"))),
    };
    let waiting: Vec<Value> = pending(api, repo, run)
        .await?
        .into_iter()
        .filter(|deployment| chosen(deployment, args.list("environments")))
        .collect();
    if waiting.is_empty() {
        return Ok(json!({ "reviewed": [], "already": true }));
    }
    if waiting.iter().any(|deployment| deployment["can_approve"] == false) {
        return Err(ActionError::permanent("this account cannot review the deployment"));
    }
    let ids: Vec<&Value> = waiting.iter().map(|deployment| &deployment["environment_id"]).collect();
    let body =
        json!({ "environment_ids": ids, "state": state, "comment": args.optional_text("comment").unwrap_or_default() });
    api.send(Method::Post, &format!("/repos/{repo}/actions/runs/{run}/pending_deployments"), &body).await?;
    let names: Vec<&Value> = waiting.iter().map(|deployment| &deployment["environment"]).collect();
    Ok(json!({ "reviewed": names, "already": false }))
}
