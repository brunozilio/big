import { fail, isObject, optionalFlag, optionalNumber, optionalText, sameAscii, textAt } from "sdk/connectors/index.ts";
import { items } from "sdk/connectors/mcp.ts";
import { type Api, jqlKey, jqlText, search as searchJql, tool } from "./api.ts";
import { quietly } from "./schema.ts";
import { summary } from "./tracker.ts";

export const PICK_TYPES = ["Story", "História", "Historia", "Bug", "Defeito", "Spike"];
const FIELDS = ["summary", "status", "issuetype"];
const DEFAULT_LIMIT = 50;
const KEY = /^[A-Za-z][A-Za-z0-9_]*-\d+$/;
const NUMBER = /^\d+$/;

type Args = Record<string, any>;

export function pickTypes(api: Api): string[] {
  const configured = api.ctx.config?.pick_types;
  const listed = Array.isArray(configured) ? configured.filter((name) => typeof name === "string" && name !== "") : [];
  return listed.length > 0 ? listed : PICK_TYPES;
}

export function keyOf(query: string, project: string | undefined): string | undefined {
  const trimmed = query.trim();
  if (KEY.test(trimmed)) return trimmed.toUpperCase();
  if (NUMBER.test(trimmed) && project !== undefined) return `${project.toUpperCase()}-${trimmed}`;
  return undefined;
}

function pickable(api: Api, issue): boolean {
  if (issue?.fields?.issuetype?.subtask === true) return false;
  const kind = textAt(issue, "/fields/issuetype/name");
  return pickTypes(api).some((name) => sameAscii(name, kind));
}

function direct(api: Api, key: string): any[] {
  const found = quietly(() => tool(api, "getJiraIssue", { issueIdOrKey: key, fields: FIELDS, responseContentFormat: "markdown" }), null);
  return isObject(found) && textAt(found, "/key") !== "" ? [found] : [];
}

function sprintKeys(api: Api, keys: string[]): Set<string> {
  if (keys.length === 0) return new Set();
  const jql = `key in (${keys.map((key) => `"${jqlKey(key)}"`).join(", ")}) AND sprint in openSprints()`;
  const page = quietly(() => searchJql(api, jql, ["summary"]), null);
  return new Set(items(page, "issues").map((issue) => textAt(issue, "/key")));
}

function jqlOf(project: string | undefined, args: Args, pick: boolean): string {
  const clauses: string[] = [];
  if (project !== undefined) clauses.push(`project = "${jqlKey(project)}"`);
  const status = optionalText(args, "status");
  if (status !== undefined) clauses.push(`status = "${jqlText(status)}"`);
  const query = jqlText(optionalText(args, "query") ?? "");
  if (query !== "") clauses.push(`text ~ "${query}"`);
  if (clauses.length === 0) fail("permanent", "a search needs a project, a status or a query");
  if (pick) clauses.push("issuetype not in subTaskIssueTypes()");
  return `${clauses.join(" AND ")} ORDER BY updated DESC`;
}

function described(api: Api, issue, sprint: Set<string>, pick: boolean) {
  const found = summary(api, issue);
  const kind = textAt(issue, "/fields/issuetype/name");
  const typed = kind === "" ? found : { ...found, type: kind };
  return pick ? { ...typed, sprint: sprint.has(found.ref) } : typed;
}

export function search(api: Api, args: Args) {
  const project = optionalText(args, "project") ?? api.project;
  const pick = optionalFlag(args, "pick") ?? false;
  const key = keyOf(optionalText(args, "query") ?? "", project);
  const exact = key === undefined ? [] : direct(api, key);
  const byKey = key !== undefined && NUMBER.test((optionalText(args, "query") ?? "").trim()) === false;
  const page = byKey ? null : searchJql(api, jqlOf(project, args, pick), FIELDS, optionalText(args, "cursor"));
  const limit = Math.min(Math.max(optionalNumber(args, "limit") ?? DEFAULT_LIMIT, 1), 100);
  const seen = new Set(exact.map((issue) => textAt(issue, "/key")));
  const rest = items(page, "issues").filter((issue) => !seen.has(textAt(issue, "/key")));
  const all = [...exact, ...rest].filter((issue) => !pick || pickable(api, issue)).slice(0, limit);
  const sprint = pick ? sprintKeys(api, all.map((issue) => textAt(issue, "/key"))) : new Set<string>();
  const issues = all.map((issue) => described(api, issue, sprint, pick));
  const token = isObject(page) && typeof page.nextPageToken === "string" && page.isLast !== true ? page.nextPageToken : null;
  return { issues, cursor: token };
}
