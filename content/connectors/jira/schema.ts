import { kindOf as errorKind, sameAscii, textAt } from "sdk/connectors/index.ts";
import { items } from "sdk/connectors/mcp.ts";
import type { Api } from "./api.ts";
import { ISSUE_TYPES, STATUSES, foundList, idOf, read } from "./operations.ts";

export const PHASES = ["todo", "doing", "done"];

export type Status = { id: string; name: string; category: string | null };
export type IssueType = { id: string; name: string; level: number; children: string[]; statuses: Status[] };

export function phaseOf(status): string | null {
  const nested = textAt(status, "/statusCategory/key");
  const key = (nested === "" ? textAt(status, "/statusCategory") : nested).toUpperCase();
  if (key === "TODO" || key === "NEW") return "todo";
  if (key === "IN_PROGRESS" || key === "INDETERMINATE") return "doing";
  if (key === "DONE") return "done";
  return null;
}

function statusOf(status): Status {
  const id = idOf(status, "/id");
  const name = textAt(status, "/name");
  return { id: id === "" ? name : id, name, category: phaseOf(status) };
}

function rankOf(kind): number {
  const level = kind?.hierarchyLevel;
  if (typeof level === "number") return level;
  return kind?.subtask === true ? -1 : 0;
}

export function quietly<T>(work: () => T, fallback: T): T {
  try {
    return work();
  } catch (error) {
    if (errorKind(error) !== "permanent") throw error;
    return fallback;
  }
}

function statusesByType(api: Api, project: string): any[] {
  return quietly(() => foundList(read(api, STATUSES, { projectIdOrKey: project })[1], ["values", "statuses", "issueTypes"]), []);
}

function statusesFor(listed: any[], kind): Status[] {
  const id = idOf(kind, "/id");
  const name = textAt(kind, "/name");
  const entry = listed.find((found) => (id !== "" && idOf(found, "/id") === id) || sameAscii(textAt(found, "/name"), name));
  return items(entry ?? null, "statuses").map(statusOf).filter((status) => status.name !== "");
}

export type Schema = { types: IssueType[]; default?: string };

export function schema(api: Api, project: string): Schema {
  const kinds = foundList(read(api, ISSUE_TYPES, { projectIdOrKey: project, maxResults: 200 })[1], ["issueTypes", "values"]);
  const named = kinds.filter((kind) => textAt(kind, "/name") !== "");
  const ranks = [...new Set(named.map(rankOf))].sort((left, right) => right - left);
  const listed = statusesByType(api, project);
  const shaped = named.map((kind) => ({ kind, level: ranks.indexOf(rankOf(kind)) + 1 }));
  const types = shaped.map(({ kind, level }) => ({
    id: idOf(kind, "/id"),
    name: textAt(kind, "/name"),
    level,
    children: shaped.filter((other) => other.level === level + 1).map((other) => textAt(other.kind, "/name")),
    statuses: statusesFor(listed, kind),
  }));
  const level = named.filter((kind) => rankOf(kind) === 0);
  const task = level.find((kind) => ["/untranslatedName", "/name"].some((place) => sameAscii(textAt(kind, place), "Task")));
  const standard = task ?? level[0] ?? named[0];
  const found: Schema = { types: types.sort((left, right) => left.level - right.level) };
  if (standard !== undefined) found.default = textAt(standard, "/name");
  return found;
}

export function childType(found: Schema, parent: string): string | undefined {
  return found.types.find((type) => sameAscii(type.name, parent))?.children[0];
}
