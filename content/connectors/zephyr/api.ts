import { NO_PROJECT, blocked, fail, idAt, optionalText } from "sdk/connectors/index.ts";

export const PAGE = 50;
export const MOST_PAGES = 10;

export type Api = { tool: (name: string, args: unknown) => any; project?: string; ctx: any };

export const idOf = (value: unknown, path: string[]) => idAt(value, path);

export const lastPage = (page: any, fetched: number) => page?.isLast === true || fetched < PAGE;

export function validKey(key: string): string {
  if (key !== "" && /^[A-Za-z0-9_-]+$/.test(key)) return key;
  return fail("permanent", `${JSON.stringify(key)} is not a project or test key`);
}

export function projectFor(api: Api, args: Record<string, unknown>): string {
  const issue = optionalText(args, "issue");
  const at = issue === undefined ? -1 : issue.lastIndexOf("-");
  const fromIssue = issue !== undefined && at > 0 ? issue.slice(0, at) : undefined;
  const project = optionalText(args, "project") ?? api.project ?? fromIssue;
  return validKey(project ?? blocked(NO_PROJECT));
}
