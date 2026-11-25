import { idAt, kindOf } from "sdk/connectors/index.ts";
import { items, payload } from "sdk/connectors/mcp.ts";
import { type Api, raw, tool } from "./api.ts";

const NAME_KEYS = ["name", "operation", "operationName", "operationId"];
const READ_VERBS = ["get", "list", "search", "find", "lookup", "fetch"];
const LIST_VERBS = ["list", "search"];
const SCOPES = ["result", "data", "response"];

export type Operation = { name: string; topic: string; query: string };

export const PROJECTS: Operation = { name: "listJiraProjects", topic: "project", query: "list jira projects visible to the user" };
export const TRANSITIONS: Operation = { name: "listJiraIssueTransitions", topic: "transition", query: "list transitions of a jira issue" };
export const STATUSES: Operation = { name: "listJiraStatuses", topic: "status", query: "list the statuses of a jira project" };
export const ISSUE_TYPES: Operation = {
  name: "listJiraProjectIssueTypesMetadata",
  topic: "type",
  query: "list the issue types of a jira project",
};
export const TYPE_FIELDS: Operation = { name: "getJiraIssueTypeMetaWithFields", topic: "field", query: "get the fields of a jira issue type" };

export const readNamed = (api: Api, name: string, inputs: Record<string, unknown>) => tool(api, "executeRead", { name, inputs });

export function read(api: Api, operation: Operation, inputs: Record<string, unknown>): [string, any] {
  let refused;
  try {
    return [operation.name, readNamed(api, operation.name, inputs)];
  } catch (error) {
    if (kindOf(error) !== "permanent") throw error;
    refused = error;
  }
  const found = raw(api, "discover", { query: operation.query });
  const name = discovered(found, operation.topic, operation.name);
  if (name === undefined) throw refused;
  return [name, readNamed(api, name, inputs)];
}

function operationName(word: string): boolean {
  const lower = word.toLowerCase();
  return (
    word.length >= 6 &&
    /^[A-Za-z0-9_]+$/.test(word) &&
    /^[a-z]/.test(word) &&
    /[A-Z]/.test(word) &&
    READ_VERBS.some((verb) => lower.startsWith(verb))
  );
}

function collect(value, found: [string, string][]) {
  if (Array.isArray(value)) {
    value.forEach((inner) => collect(inner, found));
  } else if (value !== null && typeof value === "object") {
    const named = NAME_KEYS.map((key) => value[key]).find((name) => typeof name === "string" && operationName(name));
    if (named !== undefined) found.push([named, JSON.stringify(value).toLowerCase()]);
    Object.values(value).forEach((inner) => collect(inner, found));
  } else if (typeof value === "string") {
    for (const line of value.split("\n")) {
      for (const word of line.split(/[^A-Za-z0-9_]/).filter(operationName)) found.push([word, line.toLowerCase()]);
    }
  }
}

export function discovered(value, topic: string, tried: string): string | undefined {
  const found: [string, string][] = [];
  collect(payload(value), found);
  const kept = found.filter(([name, text]) => name !== tried && text.includes(topic));
  const score = (name: string) => {
    const lower = name.toLowerCase();
    return (lower.includes(topic) ? 2 : 0) + (LIST_VERBS.some((verb) => lower.startsWith(verb)) ? 1 : 0);
  };
  if (kept.length === 0) return undefined;
  const best = Math.max(...kept.map(([name]) => score(name)));
  return kept.map(([name]) => name).find((name) => score(name) === best);
}

const scopesOf = (value) => [value, ...SCOPES.map((scope) => (value !== null && typeof value === "object" && !Array.isArray(value) ? value[scope] : undefined)).filter((scope) => scope !== undefined)];

export function foundList(value, fields: string[]): any[] {
  for (const scope of scopesOf(payload(value))) {
    const found = fields.map((field) => items(scope, field)).find((list) => list.length > 0);
    if (found !== undefined) return found;
  }
  return [];
}

export const lastPage = (value) => scopesOf(value).some((scope) => scope?.isLast === true);

export const idOf = idAt;
