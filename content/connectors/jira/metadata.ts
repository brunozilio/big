import { fail, isObject, kindOf as errorKind, optionalText, sameAscii, textAt } from "sdk/connectors/index.ts";
import { items } from "sdk/connectors/mcp.ts";
import { type Api, jqlKey, projectOf, search, tool } from "./api.ts";
import { ISSUE_TYPES, STATUSES, TYPE_FIELDS, foundList, idOf, read } from "./operations.ts";
import { childType, schema } from "./schema.ts";

const FILLED = ["project", "issuetype", "summary", "description", "parent", "labels"];
const SAMPLE_FIELDS = ["status", "issuetype"];

type Args = Record<string, any>;

function parentType(api: Api, parent: string): string {
  const answer = tool(api, "getJiraIssue", { issueIdOrKey: jqlKey(parent), fields: ["issuetype"] });
  return textAt(answer, "/fields/issuetype/name");
}

function declaredType(api: Api, project: string, parent: string | undefined): string | undefined {
  try {
    const found = schema(api, project);
    return parent === undefined ? found.default : childType(found, parentType(api, parent));
  } catch (error) {
    if (errorKind(error) !== "permanent") throw error;
    return undefined;
  }
}

export function kindOf(api: Api, project: string, args: Args): string {
  const given = optionalText(args, "type");
  if (given !== undefined) return given;
  const parent = optionalText(args, "parent");
  const fallback = parent !== undefined ? "Subtask" : "Task";
  return declaredType(api, project, parent) ?? fallback;
}

function category(status) {
  const nested = textAt(status, "/statusCategory/key");
  const key = nested === "" ? textAt(status, "/statusCategory") : nested;
  switch (key.toUpperCase()) {
    case "":
      return null;
    case "TODO":
    case "NEW":
      return "new";
    case "IN_PROGRESS":
    case "INDETERMINATE":
      return "indeterminate";
    case "DONE":
      return "done";
    default:
      return key;
  }
}

function remember(seen: Args[], status) {
  const id = idOf(status, "/id");
  const name = textAt(status, "/name");
  const known = (found: Args) => (id === "" ? found.name === name : found.id === id);
  if (name !== "" && !seen.some(known)) seen.push({ id: id === "" ? name : id, name, category: category(status) });
}

function listedStatuses(answer): Args[] {
  const seen: Args[] = [];
  for (const entry of foundList(answer, ["values", "statuses", "issueTypes"])) {
    const inner = items(entry, "statuses");
    if (inner.length === 0) remember(seen, entry);
    else inner.forEach((status) => remember(seen, status));
  }
  return seen;
}

function sampled(api: Api, project: string, transitions: (api: Api, issue: string) => any[]): Args[] {
  const page = search(api, `project = "${project}" ORDER BY updated DESC`, SAMPLE_FIELDS);
  const issues = items(page, "issues");
  const seen: Args[] = [];
  issues.forEach((issue) => remember(seen, issue?.fields?.status));
  const kinds: string[] = [];
  for (const issue of issues) {
    const kind = textAt(issue, "/fields/issuetype/name");
    if (kinds.includes(kind)) continue;
    kinds.push(kind);
    transitions(api, textAt(issue, "/key")).forEach((step) => remember(seen, step?.to));
  }
  return seen;
}

export function statuses(api: Api, project: string, transitions: (api: Api, issue: string) => any[]) {
  let found: Args[] = [];
  try {
    const [, answer] = read(api, STATUSES, { projectIdOrKey: project });
    found = listedStatuses(answer);
  } catch (error) {
    if (errorKind(error) !== "permanent") throw error;
  }
  return found.length > 0 ? found : sampled(api, project, transitions);
}

function issueType(api: Api, project: string, kind: string): string {
  const [, answer] = read(api, ISSUE_TYPES, { projectIdOrKey: project, maxResults: 200 });
  const named = (found) => ["/name", "/untranslatedName"].some((place) => sameAscii(textAt(found, place), kind));
  const found = foundList(answer, ["issueTypes", "values"]).find(named);
  return found === undefined ? fail("permanent", `${project} has no issue type ${kind}`) : idOf(found, "/id");
}

function fieldKey(field): string {
  const key = textAt(field, "/key");
  return key === "" ? textAt(field, "/fieldId") : key;
}

function required(api: Api, project: string, kind: string): Args[] {
  const issueTypeId = issueType(api, project, kind);
  const inputs = { projectIdOrKey: project, issueTypeId, requiredFieldsOnly: true, maxResults: 200 };
  const [, answer] = read(api, TYPE_FIELDS, inputs);
  return foundList(answer, ["fields", "values"])
    .filter((field) => field?.required !== false && field?.hasDefaultValue !== true)
    .filter((field) => !FILLED.includes(fieldKey(field)));
}

function choice(value) {
  const name = ["/name", "/value", "/key"].map((place) => textAt(value, place)).find((found) => found !== "");
  return { id: idOf(value, "/id"), name: name ?? "" };
}

function fieldOf(field) {
  const values = items(field, "allowedValues").map(choice);
  const found: Args = { key: fieldKey(field), name: textAt(field, "/name"), type: textAt(field, "/schema/type"), required: true };
  if (values.length > 0) found.values = values;
  return found;
}

export function fields(api: Api, args: Args) {
  const project = projectOf(api, args);
  return required(api, project, kindOf(api, project, args)).map(fieldOf);
}

const NUMBER = /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/;

function one(kind: string, choices: boolean, text: string) {
  if (choices) return { id: text };
  if (kind === "user") return { accountId: text };
  if (kind === "number") return NUMBER.test(text.trim()) ? Number(text.trim()) : text;
  return text;
}

function shaped(field, value) {
  if (field === undefined || typeof value !== "string") return value;
  const choices = items(field, "allowedValues").length > 0;
  const kind = textAt(field, "/schema/type");
  if (kind === "array") return [one(textAt(field, "/schema/items"), choices, value)];
  return one(kind, choices, value);
}

export function extra(api: Api, project: string, args: Args) {
  const given = args?.fields;
  if (!isObject(given) || Object.keys(given).length === 0) return null;
  const known = required(api, project, kindOf(api, project, args));
  const sent: Args = {};
  for (const [key, value] of Object.entries(given)) sent[key] = shaped(known.find((field) => fieldKey(field) === key), value);
  return sent;
}
