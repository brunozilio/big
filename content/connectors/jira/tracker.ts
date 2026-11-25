import {
  fail,
  flag,
  idempotencyLabel,
  isObject,
  listOf,
  optionalNumber,
  optionalText,
  sameAscii,
  text,
  textAt,
} from "sdk/connectors/index.ts";
import { items } from "sdk/connectors/mcp.ts";
import { adfToMarkdown } from "./adf.ts";
import { type Api, browse, jqlKey, jqlText, projectOf, search as searchJql, tool } from "./api.ts";
import { extra, kindOf } from "./metadata.ts";
import { TRANSITIONS, foundList, idOf, read as readOperation } from "./operations.ts";
import { visibleProjects } from "./projects.ts";
import { PHASES, phaseOf, schema } from "./schema.ts";

const ISSUE_FIELDS = [
  "summary",
  "project",
  "description",
  "status",
  "issuetype",
  "assignee",
  "labels",
  "attachment",
  "subtasks",
  "parent",
  "comment",
  "created",
  "updated",
];
const SUMMARY_FIELDS = ["summary", "status"];
const DEFAULT_LIMIT = 50;
const LANDED = ["/status/name", "/fields/status/name", "/landedStatus/name", "/landedStatus", "/toStatus/name", "/status"];

type Args = Record<string, any>;

export function markdown(value): string {
  if (typeof value === "string") return value;
  if (value === null || value === undefined) return "";
  return adfToMarkdown(value);
}

export function person(value) {
  const name = textAt(value, "/displayName");
  if (name === "") return null;
  const id = textAt(value, "/accountId");
  return { id: id === "" ? name : id, name };
}

export function statusName(issue): string {
  const name = textAt(issue, "/fields/status/name");
  return name === "" ? textAt(issue, "/fields/status") : name;
}

export function summary(api: Api, issue) {
  const key = textAt(issue, "/key");
  return { ref: key, title: textAt(issue, "/fields/summary"), status: statusName(issue), url: browse(api, key) };
}

export function transitions(api: Api, issue: string): any[] {
  const [, page] = readOperation(api, TRANSITIONS, { issueIdOrKey: issue });
  return foundList(page, ["transitions"]);
}

function findIssue(api: Api, project: string, key: string) {
  const jql = `project = "${project}" AND labels = "${idempotencyLabel(api.ctx, key)}" ORDER BY created DESC`;
  return items(searchJql(api, jql, SUMMARY_FIELDS), "issues")[0];
}

export function createIssue(api: Api, args: Args, key: string) {
  const project = projectOf(api, args);
  const title = text(args, "title");
  const found = key === "" ? undefined : findIssue(api, project, key);
  if (found !== undefined) return { ...summary(api, found), created: false };
  const parent = optionalText(args, "parent");
  const kind = kindOf(api, project, args);
  const more = extra(api, project, { ...args, type: kind });
  const labels = [...listOf(args, "labels")];
  if (key !== "") labels.push(idempotencyLabel(api.ctx, key));
  const sent: Args = { projectKey: project, summary: title, issueType: kind, description: optionalText(args, "description") ?? "", contentFormat: "markdown" };
  if (parent !== undefined) sent.parent = parent;
  if (labels.length > 0) sent.labels = labels;
  if (more !== null) sent.additional_fields = more;
  const answer = tool(api, "createJiraIssue", sent);
  const made: Args = { ...summary(api, answer), title, created: true };
  const id = idOf(answer, "/id");
  if (id !== "") made.id = id;
  return made;
}

export function commentOf(comment) {
  const id = idOf(comment, "/id");
  return {
    id: id === "" ? idOf(comment, "/commentId") : id,
    author: person(comment?.author),
    body: markdown(comment?.body),
    at: textAt(comment, "/created"),
  };
}

function comments(issue) {
  const embedded = items(issue?.fields?.comment ?? null, "comments");
  const found = embedded.length > 0 ? embedded : items(issue?.comments ?? null, "comments");
  return found.map(commentOf);
}

export function read(api: Api, issue: string, fields: string[]) {
  return tool(api, "getJiraIssue", { issueIdOrKey: jqlKey(issue), fields, view: "full", responseContentFormat: "markdown" });
}

function flagOf(fields, field: string) {
  const direct = fields?.[field];
  return direct !== null && direct !== undefined ? direct : fields?.customFields?.[field];
}

function subtask(api: Api, task) {
  const type = textAt(task, "/fields/issuetype/name");
  const found = { ...summary(api, task), done: textAt(task, "/fields/status/statusCategory/key") === "done" };
  return type === "" ? found : { ...found, type };
}

const attachment = (file) => ({
  id: textAt(file, "/id"),
  name: textAt(file, "/filename"),
  url: textAt(file, "/content"),
  mime: file?.mimeType ?? null,
  size: file?.size ?? null,
});

export function issue(api: Api, key: string) {
  const answer = read(api, key, [...ISSUE_FIELDS, api.flagField]);
  const fields = answer?.fields ?? null;
  const kind = textAt(fields, "/issuetype/name");
  const project = textAt(fields, "/project/key");
  const flags = flagOf(fields, api.flagField);
  return {
    ...summary(api, answer),
    id: idOf(answer, "/id"),
    description: markdown(fields?.description),
    type: kind === "" ? null : kind,
    parent: textAt(fields, "/parent/key") || null,
    project: project === "" ? null : project,
    labels: Array.isArray(fields?.labels) ? fields.labels : [],
    assignee: person(fields?.assignee),
    impediment: { on: Array.isArray(flags) && flags.length > 0, reason: null },
    subtasks: items(fields, "subtasks").map((task) => subtask(api, task)),
    attachments: items(fields, "attachment").map(attachment),
    comments: comments(answer),
    updated: textAt(fields, "/updated"),
  };
}

const sameText = (left: string, right: string) => jqlText(left).toLowerCase() === jqlText(right).toLowerCase();

function posted(api: Api, issue: string) {
  const page = tool(api, "listJiraIssueComments", { issueIdOrKey: issue, orderBy: "-created", maxResults: 100, responseContentFormat: "markdown" });
  return foundList(page, ["comments", "values"]).map(commentOf);
}

export function comment(api: Api, issue: string, body: string, key: string) {
  const checked = jqlKey(issue);
  const found = key === "" ? undefined : posted(api, checked).find((known) => sameText(known.body, body));
  if (found !== undefined) return { ...found, created: false };
  const answer = tool(api, "addOrEditJiraIssueComment", { issueIdOrKey: checked, commentBody: body, contentFormat: "markdown" });
  return { ...commentOf(answer), body, created: true };
}

const landed = (answer) => LANDED.map((place) => textAt(answer, place)).find((name) => name !== "") ?? "";

const HOPS = 6;

export type Target = { status?: string; category?: string };

const named = (target: Target) => target.status ?? target.category ?? "";

function reached(name: string, phase: string | null, target: Target): boolean {
  return target.status !== undefined ? sameAscii(name, target.status) : phase !== null && phase === target.category;
}

function goalOf(api: Api, key: string, target: Target): string | null {
  if (target.category !== undefined) return target.category;
  const fields = read(api, key, ["issuetype", "project"])?.fields ?? null;
  const found = schema(api, textAt(fields, "/project/key"));
  const kind = found.types.find((type) => sameAscii(type.name, textAt(fields, "/issuetype/name")));
  const all = kind === undefined ? found.types.flatMap((type) => type.statuses) : kind.statuses;
  return all.find((status) => sameAscii(status.name, target.status ?? ""))?.category ?? null;
}

function distance(phase: string | null, goal: string | null): number {
  const at = PHASES.indexOf(phase ?? "");
  const to = PHASES.indexOf(goal ?? "");
  return at < 0 || to < 0 ? PHASES.length : Math.abs(to - at);
}

function towards(known: any[], now: string | null, goal: string | null, visited: string[], lateral: boolean) {
  const from = distance(now, goal);
  const open = known.filter((step) => !visited.includes(textAt(step, "/to/name").toLowerCase()));
  const scored = open.map((step) => ({ step, left: distance(phaseOf(step?.to), goal) })).filter(({ left }) => left < from || (lateral && left === from && from < PHASES.length));
  scored.sort((left, right) => left.left - right.left);
  return scored[0]?.step;
}

function direct(known: any[], target: Target) {
  const found = known.find((step) => reached(textAt(step, "/to/name"), phaseOf(step?.to), target));
  if (found !== undefined || target.status === undefined) return found;
  return known.find((step) => sameAscii(textAt(step, "/name"), target.status ?? ""));
}

export function transition(api: Api, issue: string, target: Target) {
  const key = jqlKey(issue);
  if (named(target) === "") fail("permanent", "a transition needs a status or a category");
  const current = read(api, key, ["status"]);
  let now = statusName(current);
  let phase = phaseOf(current?.fields?.status);
  if (reached(now, phase, target)) return { ref: key, status: now, changed: false };
  const visited = [now.toLowerCase()];
  let goal: string | null | undefined;
  for (let hop = 0; hop < HOPS; hop++) {
    const known = transitions(api, key);
    let found = direct(known, target);
    if (found === undefined && known.length > 0) {
      goal = goal === undefined ? goalOf(api, key, target) : goal;
      found = towards(known, phase, goal, visited, target.status !== undefined);
    }
    if (found === undefined) fail("permanent", `${key} cannot go from ${now} to ${named(target)}`);
    const answer = tool(api, "transitionJiraIssue", { issueIdOrKey: key, transitionId: idOf(found, "/id") });
    now = landed(answer) || textAt(found, "/to/name") || named(target);
    phase = phaseOf(found?.to);
    if (reached(now, phase, target) || direct([found], target) !== undefined) return { ref: key, status: now, changed: true };
    visited.push(now.toLowerCase());
  }
  return fail("permanent", `${key} did not reach ${named(target)} in ${HOPS} transitions`);
}

export function setImpediment(api: Api, args: Args, key: string) {
  const issue = jqlKey(text(args, "issue"));
  const on = flag(args, "on");
  tool(api, "editJiraIssue", { issueIdOrKey: issue, fields: { [api.flagField]: on ? [{ value: "Impediment" }] : null } });
  const reason = on ? optionalText(args, "reason") : undefined;
  if (reason !== undefined) comment(api, issue, reason, key);
  return { on, reason: reason ?? null };
}

export function search(api: Api, args: Args) {
  const clauses: string[] = [];
  const project = optionalText(args, "project") ?? api.project;
  if (project !== undefined) clauses.push(`project = "${jqlKey(project)}"`);
  const status = optionalText(args, "status");
  if (status !== undefined) clauses.push(`status = "${jqlText(status)}"`);
  const query = jqlText(optionalText(args, "query") ?? "");
  if (query !== "") clauses.push(`text ~ "${query}"`);
  if (clauses.length === 0) fail("permanent", "a search needs a project, a status or a query");
  const jql = `${clauses.join(" AND ")} ORDER BY updated DESC`;
  const limit = Math.min(Math.max(optionalNumber(args, "limit") ?? DEFAULT_LIMIT, 1), 100);
  const page = searchJql(api, jql, SUMMARY_FIELDS, optionalText(args, "cursor"));
  const issues = items(page, "issues").slice(0, limit).map((found) => summary(api, found));
  const token = isObject(page) && typeof page.nextPageToken === "string" && page.isLast !== true ? page.nextPageToken : null;
  return { issues, cursor: token };
}

export const projects = (api: Api, args: Args) => visibleProjects(api, optionalText(args, "query"));
