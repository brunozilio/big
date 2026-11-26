import { type Creds, type Request, type Response, fail, fetch, json, missing, number, optionalText, setting, settingsList, textOf } from "sdk/connectors/index.ts";

const API_VERSION = "2022-11-28";
const RUNS = "/runs/";

export type Api = { ctx: any; base: string; token: string; creds: Creds; command: string };

export function api(ctx, creds: Creds): Api {
  const base = (setting(creds, "api") ?? ctx.config.api).replace(/\/+$/, "");
  return { ctx, base, token: creds.token, creds, command: ctx.brand?.COMMAND ?? "" };
}

export function defaultRepo(api: Api): string | undefined {
  return setting(api.creds, "repo") ?? settingsList(api.creds, "repos")[0];
}

export function request(api: Api, method: string, path: string, headers: Record<string, string> = {}): Request {
  return {
    method,
    url: `${api.base}${path}`,
    headers: {
      authorization: `Bearer ${api.token}`,
      accept: "application/vnd.github+json",
      "x-github-api-version": API_VERSION,
      ...headers,
    },
  };
}

export const raw = (api: Api, req: Request): Response => fetch(api.ctx, req);

export const get = (api: Api, path: string) => json(raw(api, request(api, "GET", path)));

export function send(api: Api, method: string, path: string, body: unknown) {
  const req = request(api, method, path, { "content-type": "application/json" });
  return json(raw(api, { ...req, body: JSON.stringify(body) }));
}

export function graphql(api: Api, query: string, variables: unknown) {
  const answer = send(api, "POST", "/graphql", { query, variables });
  const errors = Array.isArray(answer?.errors) ? answer.errors : [];
  if (errors.length > 0) return fail("permanent", textOf(errors[0], ["message"]));
  return answer?.data ?? null;
}

export function validRepo(repo: string): string {
  const part = (text: string | undefined) => !!text && text !== "." && text !== ".." && /^[A-Za-z0-9._-]+$/.test(text);
  const at = repo.indexOf("/");
  const owner = at < 0 ? undefined : repo.slice(0, at);
  const name = at < 0 ? undefined : repo.slice(at + 1);
  if (part(owner) && part(name)) return repo;
  return fail("permanent", `${JSON.stringify(repo)} is not owner/name`);
}

export const pullRef = (repo: string, number: number) => `${repo}#${number}`;
export const runRef = (repo: string, run: number) => `${repo}${RUNS}${run}`;

function numberOf(text: string, what: string): number {
  const trimmed = text.trim();
  return /^\+?\d+$/.test(trimmed) ? Number(trimmed) : fail("permanent", `${what} ${JSON.stringify(text)} is not a number`);
}

function given(args: Record<string, unknown>, name: string): string | undefined {
  const value = args?.[name];
  if (typeof value === "string" && value !== "") return value;
  if (typeof value === "number") return String(value);
  return undefined;
}

export function repoOf(api: Api, args: Record<string, unknown>): string {
  return validRepo(optionalText(args, "repo") ?? defaultRepo(api) ?? missing("repo"));
}

function split(api: Api, args: Record<string, unknown>, text: string, mark: string, what: string): [string, number] {
  const at = text.lastIndexOf(mark);
  if (at < 0) return [repoOf(api, args), numberOf(text, what)];
  return [validRepo(text.slice(0, at)), numberOf(text.slice(at + mark.length), what)];
}

export function pullOf(api: Api, args: Record<string, unknown>): [string, number] {
  const text = given(args, "pr");
  if (text !== undefined) return split(api, args, text, "#", "the pull request");
  return [repoOf(api, args), number(args, "number")];
}

export function runOf(api: Api, args: Record<string, unknown>): [string, number] {
  return split(api, args, given(args, "run") ?? missing("run"), RUNS, "the run");
}

export function state(status: unknown, conclusion: unknown): string {
  switch (conclusion) {
    case "success":
      return "passed";
    case "failure":
    case "timed_out":
    case "startup_failure":
    case "action_required":
      return "failed";
    case "cancelled":
      return "cancelled";
    case "skipped":
    case "neutral":
    case "stale":
      return "skipped";
  }
  if (status === "waiting") return "waiting";
  if (status === "in_progress") return "running";
  if (status === "completed") return "failed";
  return "queued";
}

export function verdict(state: unknown): string {
  if (state === "APPROVED") return "approve";
  if (state === "CHANGES_REQUESTED") return "request_changes";
  if (state === "DISMISSED") return "dismissed";
  if (state === "PENDING") return "pending";
  return "comment";
}

export function change(status: unknown): string {
  if (status === "added") return "added";
  if (status === "removed") return "removed";
  if (status === "renamed" || status === "copied") return "renamed";
  return "modified";
}
