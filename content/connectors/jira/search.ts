import { fail, isObject, optionalFlag, optionalNumber, optionalText, sameAscii, textAt } from "sdk/connectors/index.ts";
import { items } from "sdk/connectors/mcp.ts";
import { type Api, jqlKey, jqlText, search as searchJql, tool } from "./api.ts";
import { ISSUE_TYPES, foundList, read } from "./operations.ts";
import { quietly, rankOf } from "./schema.ts";
import { summary } from "./tracker.ts";

const FIELDS = ["summary", "status", "issuetype"];
const DEFAULT_LIMIT = 50;
const STRICT = /^([A-Za-z][A-Za-z0-9_]*)\s*[-\u2010-\u2015\u2212]\s*(\d+)$/u;
const LOOSE = /^([A-Za-z][A-Za-z0-9_]*)\s+(\d+)$/;
const LINKED = /(?:browse\/|selectedIssue=)([A-Za-z][A-Za-z0-9_]*-\d+)/;
const NUMBER = /^#?(\d+)$/;

type Args = Record<string, any>;

export type Wanted = { key: string; only: boolean };

export function keyOf(query: string, project: string | undefined): Wanted | undefined {
  const trimmed = query.trim();
  const linked = LINKED.exec(trimmed);
  if (linked !== null) return { key: linked[1].toUpperCase(), only: true };
  const strict = STRICT.exec(trimmed);
  if (strict !== null) return { key: `${strict[1]}-${strict[2]}`.toUpperCase(), only: true };
  const loose = LOOSE.exec(trimmed);
  if (loose !== null) return { key: `${loose[1]}-${loose[2]}`.toUpperCase(), only: false };
  const number = NUMBER.exec(trimmed);
  if (number !== null && project !== undefined) return { key: `${project}-${number[1]}`.toUpperCase(), only: false };
  return undefined;
}

type Ranks = Map<string, Map<string, number>>;

function projectRanks(api: Api, project: string): Map<string, number> {
  const kinds = quietly(() => foundList(read(api, ISSUE_TYPES, { projectIdOrKey: project, maxResults: 200 })[1], ["issueTypes", "values"]), []);
  return new Map(kinds.map((kind) => [textAt(kind, "/name").toLowerCase(), rankOf(kind)]));
}

function rankIn(api: Api, issue, ranks: Ranks): number | undefined {
  const kind = issue?.fields?.issuetype;
  if (typeof kind?.hierarchyLevel === "number") return kind.hierarchyLevel;
  if (kind?.subtask === true) return -1;
  const project = textAt(issue, "/key").replace(/-\d+$/, "");
  if (!ranks.has(project)) ranks.set(project, projectRanks(api, project));
  return ranks.get(project)?.get(textAt(kind, "/name").toLowerCase());
}

function pickable(api: Api, issue, exact: Set<string>, ranks: Ranks): boolean {
  const rank = rankIn(api, issue, ranks);
  if (exact.has(textAt(issue, "/key"))) return rank !== -1;
  return rank === 0;
}

function direct(api: Api, key: string): any[] {
  const read = quietly(() => tool(api, "getJiraIssue", { issueIdOrKey: key, fields: FIELDS, view: "full", responseContentFormat: "markdown" }), null);
  if (isObject(read) && textAt(read, "/key") !== "") return [read];
  const page = quietly(() => searchJql(api, `key = "${jqlKey(key)}"`, FIELDS), null);
  return items(page, "issues").filter((issue) => sameAscii(textAt(issue, "/key"), key));
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
  const wanted = keyOf(optionalText(args, "query") ?? "", project);
  const exact = wanted === undefined ? [] : direct(api, wanted.key);
  const page = wanted?.only === true ? null : searchJql(api, jqlOf(project, args, pick), FIELDS, optionalText(args, "cursor"));
  const limit = Math.min(Math.max(optionalNumber(args, "limit") ?? DEFAULT_LIMIT, 1), 100);
  const seen = new Set(exact.map((issue) => textAt(issue, "/key")));
  const rest = items(page, "issues").filter((issue) => !seen.has(textAt(issue, "/key")));
  const ranks: Ranks = new Map();
  const all = [...exact, ...rest].filter((issue) => !pick || pickable(api, issue, seen, ranks)).slice(0, limit);
  const sprint = pick ? sprintKeys(api, all.map((issue) => textAt(issue, "/key"))) : new Set<string>();
  const issues = all.map((issue) => described(api, issue, sprint, pick));
  const token = isObject(page) && typeof page.nextPageToken === "string" && page.isLast !== true ? page.nextPageToken : null;
  return { issues, cursor: token };
}
