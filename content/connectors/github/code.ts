import { ActionError, formComponent, jsonText, listIn, optionalFlag, optionalText, setting, text, textOf, fail } from "sdk/connectors/index.ts";
import { author, authored, carries, marked, strip } from "sdk/connectors/idempotency.ts";
import { type Api, change, get, graphql, pullOf, pullRef, raw, repoOf, request, send, state, verdict } from "./api.ts";

type Args = Record<string, any>;

const FILE_PAGES = 30;
const REPO_PAGE = 100;
const REPO_PAGES = 20;
const BRANCH_PAGES = 10;

const THREADS =
  "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100){nodes{id isResolved isOutdated path line comments(first:50){nodes{id body createdAt author{login}}}}}}}}";
const RESOLVE = "mutation($thread:ID!){resolveReviewThread(input:{threadId:$thread}){thread{id isResolved}}}";

const isMerged = (pull: any) => pull?.merged === true || (pull?.merged_at !== undefined && pull?.merged_at !== null);

const permanentWith = (error: unknown, text: string) =>
  error instanceof ActionError && (error.class as any).kind === "permanent" && error.message.includes(text);

export function pullSummary(api: Api, repo: string, pull: any, created: boolean) {
  const number = typeof pull?.number === "number" ? pull.number : 0;
  const merged = isMerged(pull);
  return {
    ref: pullRef(repo, number),
    repo,
    number,
    url: textOf(pull, ["html_url"]),
    title: textOf(pull, ["title"]),
    body: strip(api.command, textOf(pull, ["body"])),
    state: merged ? "merged" : textOf(pull, ["state"]),
    merged,
    draft: pull?.draft === true,
    head: textOf(pull, ["head", "ref"]),
    base: textOf(pull, ["base", "ref"]),
    sha: textOf(pull, ["head", "sha"]),
    mergeable: pull?.mergeable ?? null,
    author: textOf(pull, ["user", "login"]),
    updated: textOf(pull, ["updated_at"]),
    checks: [],
    reviews: [],
    created,
  };
}

function byBranch(api: Api, repo: string, head: string) {
  const owner = repo.split("/")[0] ?? "";
  const pulls = listIn(get(api, `/repos/${repo}/pulls?head=${formComponent(`${owner}:${head}`)}&state=all&per_page=20`));
  return pulls.find((pull) => pull.state === "open") ?? pulls.find((pull) => pull.merged_at != null);
}

function baseOf(api: Api, args: Args, repo: string): string {
  const given = optionalText(args, "base") ?? setting(api.creds, "base");
  if (given !== undefined) return given;
  const found = get(api, `/repos/${repo}`);
  return typeof found?.default_branch === "string" && found.default_branch !== "" ? found.default_branch : "main";
}

export function openPullRequest(api: Api, args: Args, key: string) {
  const repo = repoOf(api, args);
  const head = text(args, "head");
  const existing = byBranch(api, repo, head);
  if (existing) return pullSummary(api, repo, existing, false);
  const body = {
    title: text(args, "title"),
    body: marked(api.command, optionalText(args, "body") ?? "", key),
    head,
    base: baseOf(api, args, repo),
    draft: args?.draft === true,
  };
  try {
    return pullSummary(api, repo, send(api, "POST", `/repos/${repo}/pulls`, body), true);
  } catch (error) {
    if (!permanentWith(error, "already exists")) throw error;
    const found = byBranch(api, repo, head);
    if (!found) throw error;
    return pullSummary(api, repo, found, false);
  }
}

const check = (run: any) => ({ name: run?.name ?? null, state: state(run?.status, run?.conclusion), url: run?.html_url ?? null });

const review = (api: Api, found: any) => ({
  id: jsonText(found?.id),
  author: textOf(found, ["user", "login"]),
  agent: author(api.command, textOf(found, ["body"])),
  verdict: verdict(found?.state),
  body: strip(api.command, textOf(found, ["body"])),
  at: textOf(found, ["submitted_at"]),
});

export function pullRequest(api: Api, args: Args) {
  const [repo, number] = pullOf(api, args);
  const pull = get(api, `/repos/${repo}/pulls/${number}`);
  const sha = textOf(pull, ["head", "sha"]);
  const checks = get(api, `/repos/${repo}/commits/${sha}/check-runs?per_page=100`);
  const reviews = get(api, `/repos/${repo}/pulls/${number}/reviews?per_page=100`);
  const summary: any = pullSummary(api, repo, pull, false);
  summary.checks = listIn(checks?.check_runs).map(check);
  summary.reviews = listIn(reviews).map((found) => review(api, found));
  return summary;
}

const file = (found: any) => ({
  path: found?.filename ?? null,
  previous: found?.previous_filename ?? null,
  change: change(found?.status),
  additions: typeof found?.additions === "number" ? found.additions : 0,
  deletions: typeof found?.deletions === "number" ? found.deletions : 0,
  patch: textOf(found, ["patch"]),
});

export function filesDiff(api: Api, args: Args) {
  const [repo, number] = pullOf(api, args);
  const files: unknown[] = [];
  for (let page = 1; page <= FILE_PAGES; page++) {
    const listed = listIn(get(api, `/repos/${repo}/pulls/${number}/files?per_page=100&page=${page}`));
    files.push(...listed.map(file));
    if (listed.length < 100) break;
  }
  return files;
}

function threadComments(api: Api, thread: any) {
  return listIn(thread?.comments?.nodes).map((comment) => ({
    id: comment?.id ?? null,
    thread: thread?.id ?? null,
    path: thread?.path ?? null,
    line: thread?.line ?? null,
    author: textOf(comment, ["author", "login"]),
    agent: author(api.command, textOf(comment, ["body"])),
    body: strip(api.command, textOf(comment, ["body"])),
    resolved: thread?.isResolved === true,
    outdated: thread?.isOutdated === true,
    at: textOf(comment, ["createdAt"]),
  }));
}

export function reviewComments(api: Api, args: Args) {
  const [repo, number] = pullOf(api, args);
  const at = repo.indexOf("/");
  const data = graphql(api, THREADS, { owner: repo.slice(0, at), name: repo.slice(at + 1), number });
  return listIn(data?.repository?.pullRequest?.reviewThreads?.nodes).flatMap((thread) => threadComments(api, thread));
}

function reviewEvent(chosen: string): string {
  if (chosen === "approve") return "APPROVE";
  if (chosen === "comment") return "COMMENT";
  if (chosen === "request_changes") return "REQUEST_CHANGES";
  return fail("permanent", `${JSON.stringify(chosen)} is not approve, comment or request_changes`);
}

export function submitReview(api: Api, args: Args, key: string) {
  const [repo, number] = pullOf(api, args);
  const chosen = text(args, "verdict");
  const event = reviewEvent(chosen);
  if (key !== "") {
    const reviews = listIn(get(api, `/repos/${repo}/pulls/${number}/reviews?per_page=100`));
    const found = reviews.find((found) => carries(api.command, typeof found?.body === "string" ? found.body : "", key));
    if (found) return { id: jsonText(found.id), verdict: verdict(found.state), created: false };
  }
  const agent = optionalText(args, "agent");
  const comments = listIn(args?.lines).map((line) => ({
    path: line?.path ?? null,
    line: line?.line ?? null,
    body: authored(api.command, typeof line?.body === "string" ? line.body : "", agent),
    side: "RIGHT",
  }));
  const said = authored(api.command, optionalText(args, "body") ?? "", agent);
  const body: any = { event, body: marked(api.command, said, key) };
  if (comments.length > 0) body.comments = comments;
  const path = `/repos/${repo}/pulls/${number}/reviews`;
  try {
    const made = send(api, "POST", path, body);
    return { id: jsonText(made?.id), verdict: chosen, created: true };
  } catch (error) {
    if (event === "COMMENT" || !permanentWith(error, "your own pull request")) throw error;
    body.event = "COMMENT";
    const made = send(api, "POST", path, body);
    return { id: jsonText(made?.id), verdict: chosen, created: true, as: "comment" };
  }
}

export function resolveThread(api: Api, args: Args) {
  const thread = text(args, "thread");
  const data = graphql(api, RESOLVE, { thread });
  const resolved = data?.resolveReviewThread?.thread?.isResolved;
  return { thread, resolved: typeof resolved === "boolean" ? resolved : true };
}

function mergeMethod(method: string | undefined): string {
  const chosen = method ?? "merge";
  if (chosen === "merge" || chosen === "squash" || chosen === "rebase") return chosen;
  return fail("permanent", `${JSON.stringify(chosen)} is not merge, squash or rebase`);
}

function deleteHead(api: Api, repo: string, pull: any): boolean {
  const head = textOf(pull, ["head", "ref"]);
  const own = pull?.head?.repo?.full_name === repo;
  if (!own || head === "" || head === textOf(pull, ["base", "ref"])) return false;
  try {
    raw(api, request(api, "DELETE", `/repos/${repo}/git/refs/heads/${head}`));
    return true;
  } catch (_) {
    return false;
  }
}

export function merge(api: Api, args: Args) {
  const [repo, number] = pullOf(api, args);
  const method = mergeMethod(optionalText(args, "method"));
  const pull = get(api, `/repos/${repo}/pulls/${number}`);
  const remove = optionalFlag(args, "delete_branch") ?? true;
  if (isMerged(pull)) {
    const deleted = remove && deleteHead(api, repo, pull);
    return { merged: true, already: true, sha: pull?.merge_commit_sha ?? null, deleted };
  }
  if (pull?.state === "closed") return fail("permanent", `${repo}#${number} was closed without merging`);
  const done = send(api, "PUT", `/repos/${repo}/pulls/${number}/merge`, { merge_method: method, sha: textOf(pull, ["head", "sha"]) });
  const merged = typeof done?.merged === "boolean" ? done.merged : true;
  const deleted = merged && remove && deleteHead(api, repo, pull);
  return { merged, already: false, sha: done?.sha ?? null, deleted };
}

const repoSummary = (repo: any) => ({
  id: typeof repo?.id === "number" ? String(repo.id) : textOf(repo, ["node_id"]),
  ref: textOf(repo, ["full_name"]),
  name: textOf(repo, ["name"]),
  url: textOf(repo, ["html_url"]),
  default_branch: textOf(repo, ["default_branch"]),
  private: repo?.private === true,
});

export function repos(api: Api, args: Args) {
  const query = (optionalText(args, "query") ?? "").toLowerCase();
  const found: unknown[] = [];
  for (let page = 1; page <= REPO_PAGES; page++) {
    const path = `/user/repos?per_page=${REPO_PAGE}&page=${page}&sort=updated&affiliation=owner,collaborator,organization_member`;
    const listed = listIn(get(api, path));
    for (const repo of listed.map(repoSummary)) {
      if (repo.ref !== "" && (query === "" || repo.ref.toLowerCase().includes(query))) found.push(repo);
    }
    if (listed.length < REPO_PAGE) break;
  }
  return found;
}

export function pullRequests(api: Api, args: Args) {
  const repo = repoOf(api, args);
  const listed = listIn(get(api, `/repos/${repo}/pulls?state=open&sort=updated&direction=desc&per_page=100`));
  return listed.map((pull) => pullSummary(api, repo, pull, false));
}

export function branches(api: Api, args: Args) {
  const repo = repoOf(api, args);
  const fallback = textOf(get(api, `/repos/${repo}`), ["default_branch"]);
  const names: string[] = [];
  for (let page = 1; page <= BRANCH_PAGES; page++) {
    const listed = listIn(get(api, `/repos/${repo}/branches?per_page=100&page=${page}`));
    names.push(...listed.map((branch) => textOf(branch, ["name"])).filter((name) => name !== ""));
    if (listed.length < 100) break;
  }
  return names.map((name) => ({ default: name === fallback, name }));
}

function branch(args: Args, name: string): string {
  const value = text(args, name);
  const valid = !value.startsWith("-") && !value.includes("..") && !/\s/.test(value);
  return valid ? value : fail("permanent", `${JSON.stringify(value)} is not a branch`);
}

export function mergeBranches(api: Api, args: Args) {
  const repo = repoOf(api, args);
  const base = branch(args, "base");
  const head = branch(args, "head");
  const message = optionalText(args, "message") ?? `Merge ${head} into ${base}`;
  const done = send(api, "POST", `/repos/${repo}/merges`, { base, head, commit_message: message });
  if (done === null) return { merged: true, already: true };
  return { merged: true, already: false, sha: textOf(done, ["sha"]) };
}
