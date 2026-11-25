import { type Creds, iso, json, jsonText, listIn, setting, settingsList, textOf, timestamp } from "sdk/connectors/index.ts";
import { strip } from "sdk/connectors/idempotency.ts";
import { type Api, api, get, pullRef, raw, request, runRef, state, verdict } from "./api.ts";

type Cursor = { since: string; etags: Record<string, string> };

type Poll = { api: Api; since: number; newest: number; etags: Record<string, string>; events: any[] };

function changed(poll: Poll, path: string): any[] | undefined {
  const etag = poll.etags[path];
  const req = request(poll.api, "GET", path, etag === undefined ? {} : { "if-none-match": etag });
  const response = raw(poll.api, req);
  if (response.status === 304) return undefined;
  const fresh = response.headers?.etag;
  if (fresh !== undefined) poll.etags[path] = fresh;
  const body = json(response);
  const listed = body !== null && typeof body === "object" && "workflow_runs" in body ? body.workflow_runs : body;
  return listIn(listed);
}

function push(poll: Poll, capability: string, kind: string, id: string, subject: string, data: unknown, at: string) {
  const when = timestamp(at);
  if (when === undefined || when <= poll.since) return;
  poll.newest = Math.max(poll.newest, when);
  poll.events.push({ id, capability, kind, subject, data, at: iso(when), when });
}

function reviews(poll: Poll, repo: string, pull: any, subject: string) {
  const number = typeof pull?.number === "number" ? pull.number : 0;
  for (const review of listIn(get(poll.api, `/repos/${repo}/pulls/${number}/reviews?per_page=100`))) {
    const data = {
      pr: subject,
      repo,
      number,
      head: textOf(pull, ["head", "ref"]),
      base: textOf(pull, ["base", "ref"]),
      title: pull?.title ?? null,
      verdict: verdict(review?.state),
      author: textOf(review, ["user", "login"]),
      body: strip(poll.api.command, typeof review?.body === "string" ? review.body : ""),
    };
    const id = `github:${repo}:review:${jsonText(review?.id)}`;
    push(poll, "code", "code.review.submitted", id, subject, data, textOf(review, ["submitted_at"]));
  }
}

function kindOf(pull: any): [string, string, string] {
  if (typeof pull?.merged_at === "string") return ["code.pr.merged", pull.merged_at, "merged"];
  if (pull?.state === "closed") return ["code.pr.closed", textOf(pull, ["closed_at"]), "closed"];
  return ["code.pr.updated", textOf(pull, ["updated_at"]), "open"];
}

function pulls(poll: Poll, repo: string) {
  const found = changed(poll, `/repos/${repo}/pulls?state=all&sort=updated&direction=desc&per_page=50`);
  if (found === undefined) return;
  const fresh = found.filter((pull) => (timestamp(textOf(pull, ["updated_at"])) ?? -Infinity) > poll.since);
  for (const pull of fresh) {
    const number = typeof pull?.number === "number" ? pull.number : 0;
    const subject = pullRef(repo, number);
    const [kind, at, shown] = kindOf(pull);
    const head = textOf(pull, ["head", "ref"]);
    const base = textOf(pull, ["base", "ref"]);
    const data = { pr: subject, repo, number, head, base, state: shown, title: pull?.title ?? null };
    push(poll, "code", kind, `github:${repo}:pr:${number}:${kind}:${at}`, subject, data, at);
    reviews(poll, repo, pull, subject);
  }
}

function runs(poll: Poll, repo: string) {
  const found = changed(poll, `/repos/${repo}/actions/runs?per_page=50`);
  if (found === undefined) return;
  for (const run of found) {
    const kind = run?.status === "completed" ? "pipelines.run.finished" : run?.status === "waiting" ? "pipelines.deployment.waiting" : undefined;
    if (kind === undefined) continue;
    const id = `github:${repo}:run:${jsonText(run?.id)}:${jsonText(run?.run_attempt)}:${kind}`;
    const subject = runRef(repo, typeof run?.id === "number" ? run.id : 0);
    const data = {
      run: subject,
      repo,
      pipeline: run?.name ?? null,
      head: run?.head_branch ?? null,
      sha: run?.head_sha ?? null,
      state: state(run?.status, run?.conclusion),
    };
    push(poll, "pipelines", kind, id, subject, data, textOf(run, ["updated_at"]));
  }
}

function reposOf(creds: Creds): string[] {
  const repos = settingsList(creds, "repos");
  const single = setting(creds, "repo");
  if (single !== undefined) repos.push(single);
  return repos;
}

function cursorOf(text: string | null): Cursor | undefined {
  if (typeof text !== "string") return undefined;
  try {
    const parsed = JSON.parse(text);
    if (typeof parsed?.since !== "string" || timestamp(parsed.since) === undefined) return undefined;
    return { since: parsed.since, etags: parsed.etags && typeof parsed.etags === "object" ? parsed.etags : {} };
  } catch (_) {
    return undefined;
  }
}

export function poll({ creds, cursor }: { creds: Creds; cursor: string | null }, ctx) {
  const known = cursorOf(cursor);
  if (known === undefined) {
    const now = Number(ctx.clock.now()) * 1000;
    return { events: [], cursor: JSON.stringify({ since: iso(now), etags: {} }) };
  }
  const since = timestamp(known.since) ?? 0;
  const state: Poll = { api: api(ctx, creds), since, newest: since, etags: { ...known.etags }, events: [] };
  for (const repo of reposOf(creds)) {
    pulls(state, repo);
    runs(state, repo);
  }
  const events = state.events.map((event, at) => ({ event, at })).sort((a, b) => a.event.when - b.event.when || a.at - b.at);
  const listed = events.map(({ event: { when, ...event } }) => event);
  return { events: listed, cursor: JSON.stringify({ since: iso(state.newest), etags: state.etags }) };
}
