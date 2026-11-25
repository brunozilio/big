import { type Creds, fail, isObject, setting, textAt, timestampOf, timestampText } from "sdk/connectors/index.ts";
import { items } from "sdk/connectors/mcp.ts";
import { type Api, apiOf, jqlKey, search, tool } from "./api.ts";
import { commentOf } from "./tracker.ts";

const PAGES = 10;
const DETAIL = ["summary", "status", "issuetype", "parent", "created", "updated", "comment"];
const MARGIN_MS = 15 * 60 * 60 * 1000;

type Event = { id: string; capability: string; kind: string; subject: string; scope: string; data: unknown; at: string; ms: number };

const pad = (value: number) => String(value).padStart(2, "0");

export function eventsJql(projects: string[], since: number): string {
  const from = new Date(since - MARGIN_MS);
  const day = `${from.getUTCFullYear()}-${pad(from.getUTCMonth() + 1)}-${pad(from.getUTCDate())} ${pad(from.getUTCHours())}:${pad(from.getUTCMinutes())}`;
  const keys = projects.map((project) => `"${jqlKey(project)}"`).join(", ");
  return `project in (${keys}) AND updated >= "${day}" ORDER BY updated ASC`;
}

export function spaceOf(key: string): string {
  const at = key.lastIndexOf("-");
  return at > 0 ? key.slice(0, at) : "";
}

function watchedOf(creds: Creds, watch: unknown): string[] {
  const listed = Array.isArray(watch) ? watch.filter((key): key is string => typeof key === "string" && key !== "") : [];
  if (listed.length > 0) return [...new Set(listed)];
  return [setting(creds, "project") ?? fail("permanent", "no Jira space is bound to watch")];
}

const owned = (value, key: string) => (isObject(value) && Object.prototype.hasOwnProperty.call(value, key) ? value[key] : null);

function asList(value): any[] {
  return Array.isArray(value) ? value : [];
}

class Collector {
  events: Event[] = [];
  since: number;
  constructor(since: number) {
    this.since = since;
  }

  push(id: string, kind: string, subject: string, data: unknown, at: string) {
    const ms = timestampOf(at);
    if (ms === undefined || !(ms > this.since)) return;
    this.events.push({ id, capability: "tracker", kind, subject, scope: spaceOf(subject), data, at: timestampText(ms), ms });
  }

  issue(issue) {
    const key = textAt(issue, "/key");
    const fields = issue?.fields ?? null;
    const summary: Record<string, unknown> = { ref: key, title: textAt(fields, "/summary"), status: textAt(fields, "/status/name") };
    const type = textAt(fields, "/issuetype/name");
    const parent = textAt(fields, "/parent/key");
    if (type !== "") summary.type = type;
    if (parent !== "") summary.parent = parent;
    this.push(`jira:${key}:created`, "tracker.issue.created", key, summary, textAt(fields, "/created"));
    for (const history of asList(issue?.changelog?.histories)) this.history(key, history);
    for (const comment of asList(fields?.comment?.comments)) {
      this.push(`jira:${key}:comment:${textAt(comment, "/id")}`, "tracker.comment.added", key, commentOf(comment), textAt(comment, "/created"));
    }
  }

  history(key: string, history) {
    const at = textAt(history, "/created");
    const author = textAt(history, "/author/displayName");
    for (const item of asList(history?.items)) {
      const field = textAt(item, "/field");
      const flag = (name: string) => textAt(item, name).trim() !== "";
      let kind: string;
      let data: unknown;
      const lower = field.toLowerCase();
      if (lower === "status") {
        kind = "tracker.status.changed";
        data = { from: owned(item, "fromString"), to: owned(item, "toString"), author };
      } else if (lower === "flagged") {
        kind = "tracker.impediment.changed";
        data = { from: flag("/fromString"), to: flag("/toString"), author };
      } else {
        continue;
      }
      this.push(`jira:${key}:${field}:${textAt(history, "/id")}`, kind, key, data, at);
    }
  }
}

function latest(issues: any[], since: number): number {
  return issues.map((issue) => timestampOf(textAt(issue, "/fields/updated"))).reduce<number>((newest, at) => (at !== undefined && at > newest ? at : newest), since);
}

function detail(api: Api, key: string) {
  return tool(api, "getJiraIssue", { issueIdOrKey: key, fields: DETAIL, expand: "changelog", view: "full", responseContentFormat: "markdown" });
}

export function poll({ creds, cursor, watch }: { creds: Creds; cursor: string | null; watch?: unknown }, ctx) {
  const since = cursor === null || cursor === undefined ? undefined : timestampOf(cursor);
  if (since === undefined) return { events: [], cursor: timestampText(Date.now()) };
  const projects = watchedOf(creds, watch);
  const api = apiOf(ctx, creds);
  const jql = eventsJql(projects, since);
  const collector = new Collector(since);
  let newest = since;
  let token: string | undefined;
  for (let page = 0; page < PAGES; page++) {
    const found = search(api, jql, ["updated"], token);
    const issues = items(found, "issues");
    for (const issue of issues) collector.issue(detail(api, textAt(issue, "/key")));
    newest = latest(issues, newest);
    token = isObject(found) && typeof found.nextPageToken === "string" && found.isLast !== true ? found.nextPageToken : undefined;
    if (token === undefined) break;
  }
  const events = [...collector.events].sort((left, right) => left.ms - right.ms).map(({ ms, ...event }) => event);
  return { events, cursor: timestampText(newest) };
}
