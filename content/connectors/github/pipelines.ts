import { ActionError, fail, jsonText, listIn, number, optionalNumber, optionalText, query, text, textOf } from "sdk/connectors/index.ts";
import { type Api, get, raw, repoOf, request, runOf, runRef, send, state } from "./api.ts";
import { type WorkflowJob, jobs, matches } from "./workflow.ts";

type Args = Record<string, any>;

const DEFAULT_TAIL = 500;
const DEFAULT_RUNS = 20;

const finishedAt = (run: any, shown: string) =>
  ["passed", "failed", "cancelled", "skipped"].includes(shown) ? (run?.updated_at ?? null) : null;

export function runSummary(repo: string, run: any) {
  const id = typeof run?.id === "number" ? run.id : 0;
  const shown = state(run?.status, run?.conclusion);
  const started = typeof run?.run_started_at === "string" ? run.run_started_at : typeof run?.created_at === "string" ? run.created_at : null;
  return {
    ref: runRef(repo, id),
    repo,
    pipeline: textOf(run, ["name"]),
    file: textOf(run, ["path"]),
    event: textOf(run, ["event"]),
    actor: textOf(run, ["triggering_actor", "login"]),
    head: textOf(run, ["head_branch"]),
    sha: textOf(run, ["head_sha"]),
    state: shown,
    attempt: typeof run?.run_attempt === "number" ? run.run_attempt : 1,
    url: textOf(run, ["html_url"]),
    started,
    finished: finishedAt(run, shown),
  };
}

const shaLike = (value: string) => /^[0-9a-fA-F]{40}$/.test(value);

function runsPath(repo: string, args: Args): string {
  const limit = Math.min(100, Math.max(1, optionalNumber(args, "limit") ?? DEFAULT_RUNS));
  const file = optionalText(args, "pipeline") ?? optionalText(args, "workflow");
  let base = `/repos/${repo}/actions/runs`;
  if (file !== undefined) {
    if (!/^[A-Za-z0-9._-]+$/.test(file)) return fail("permanent", `${JSON.stringify(file)} is not a pipeline file or id`);
    base = `/repos/${repo}/actions/workflows/${file}/runs`;
  }
  const pairs: [string, string][] = [["per_page", String(limit)]];
  const reference = optionalText(args, "ref");
  if (reference !== undefined) pairs.push([shaLike(reference) ? "head_sha" : "branch", reference]);
  const status = optionalText(args, "status");
  if (status !== undefined) pairs.push(["status", status]);
  return `${base}?${query(pairs)}`;
}

export function runs(api: Api, args: Args) {
  const repo = repoOf(api, args);
  const answer = get(api, runsPath(repo, args));
  return listIn(answer?.workflow_runs).map((run) => runSummary(repo, run));
}

function workflowJobs(api: Api, repo: string, run: any): WorkflowJob[] {
  const path = textOf(run, ["path"]).split("@")[0] ?? "";
  if (path === "") return [];
  const sha = textOf(run, ["head_sha"]);
  let found: any;
  try {
    found = get(api, `/repos/${repo}/contents/${path}?ref=${sha}`);
  } catch (error) {
    if (error instanceof ActionError && (error.class as any).kind === "permanent") return [];
    throw error;
  }
  const encoded = textOf(found, ["content"]).replace(/\s/g, "");
  let yaml = "";
  try {
    yaml = api.ctx.encoding.convert({ data: encoded, from: "base64", to: "text" });
  } catch (_) {
    yaml = "";
  }
  return jobs(yaml);
}

const stepView = (step: any) => ({
  name: textOf(step, ["name"]),
  state: state(step?.status, step?.conclusion),
  started: step?.started_at ?? null,
  finished: step?.completed_at ?? null,
});

function jobView(job: any, graph: WorkflowJob[]) {
  const name = textOf(job, ["name"]);
  const known = graph.find((candidate) => matches(candidate, name));
  return {
    id: jsonText(job?.id),
    name,
    key: known?.key ?? null,
    needs: known?.needs ?? [],
    state: state(job?.status, job?.conclusion),
    started: job?.started_at ?? null,
    finished: job?.completed_at ?? null,
    url: job?.html_url ?? null,
    steps: listIn(job?.steps).map(stepView),
  };
}

export function run(api: Api, args: Args) {
  const [repo, id] = runOf(api, args);
  const found = get(api, `/repos/${repo}/actions/runs/${id}`);
  const listed = get(api, `/repos/${repo}/actions/runs/${id}/jobs?per_page=100&filter=latest`);
  const graph = workflowJobs(api, repo, found);
  const summary: any = runSummary(repo, found);
  summary.jobs = listIn(listed?.jobs).map((job) => jobView(job, graph));
  return summary;
}

export function jobLog(api: Api, args: Args) {
  const [repo] = runOf(api, args);
  const job = number(args, "job");
  const tail = optionalNumber(args, "tail") ?? DEFAULT_TAIL;
  const log = raw(api, request(api, "GET", `/repos/${repo}/actions/jobs/${job}/logs`)).body;
  const lines = log === "" ? [] : log.replace(/\r?\n$/, "").split(/\r?\n/);
  const kept = lines.slice(Math.max(0, lines.length - tail)).join("\n");
  return { text: kept, truncated: lines.length > tail };
}

function deploymentView(deployment: any, reference: string) {
  const reviewers = listIn(deployment?.reviewers)
    .map((reviewer) => textOf(reviewer, ["reviewer", "login"]))
    .filter((login) => login !== "");
  return {
    id: jsonText(deployment?.environment?.id),
    environment: deployment?.environment?.name ?? null,
    run: reference,
    reviewers,
    canApprove: deployment?.current_user_can_approve === true,
  };
}

function pending(api: Api, repo: string, id: number) {
  const answer = get(api, `/repos/${repo}/actions/runs/${id}/pending_deployments`);
  const reference = runRef(repo, id);
  return listIn(answer).map((deployment) => deploymentView(deployment, reference));
}

export function pendingDeployments(api: Api, args: Args) {
  const [repo, id] = runOf(api, args);
  return pending(api, repo, id);
}

function chosen(deployment: any, wanted: unknown[]): boolean {
  return (
    wanted.length === 0 ||
    wanted.some((want) => {
      const named = typeof want === "string" ? want : jsonText(want);
      return deployment.id === named || deployment.environment === named;
    })
  );
}

export function reviewDeployment(api: Api, args: Args) {
  const [repo, id] = runOf(api, args);
  const decision = text(args, "decision");
  const decided = decision === "approve" ? "approved" : decision === "reject" ? "rejected" : fail("permanent", `${JSON.stringify(decision)} is not approve or reject`);
  const wanted = [...listIn(args?.deployments), ...listIn(args?.environments)];
  const waiting = pending(api, repo, id).filter((deployment) => chosen(deployment, wanted));
  if (waiting.length === 0) return { decision, deployments: [], already: true };
  if (waiting.some((deployment) => deployment.canApprove === false)) return fail("permanent", "this account cannot review the deployment");
  const ids = waiting.map((deployment) => Number(deployment.id)).filter((value) => Number.isInteger(value) && value >= 0);
  const body = { environment_ids: ids, state: decided, comment: optionalText(args, "comment") ?? "" };
  send(api, "POST", `/repos/${repo}/actions/runs/${id}/pending_deployments`, body);
  return { decision, deployments: waiting.map((deployment) => deployment.id), already: false };
}
