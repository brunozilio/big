import { type Creds, blocked, fail, optionalText, setting } from "sdk/connectors/index.ts";

export const NO_SITE = "Reconnect Jira and choose the site.";
export const NO_PROJECT = "Choose the project.";
export const NO_SITES =
  "No Jira site was found in this Atlassian account. Check that the account has access to Jira and connect again.";
export const DEFAULT_FLAG_FIELD = "customfield_10021";

export type Api = {
  ctx: any;
  url: string;
  token: string;
  cloud: string;
  site: string;
  flagField: string;
  project?: string;
};

export function apiOf(ctx, creds: Creds): Api {
  const cloud = setting(creds, "cloud_id") ?? blocked(NO_SITE);
  return {
    ctx,
    url: ctx.config.mcp_url,
    token: creds.token,
    cloud,
    site: (setting(creds, "site_url") ?? "").replace(/\/+$/, ""),
    flagField: setting(creds, "flag_field") ?? DEFAULT_FLAG_FIELD,
    project: setting(creds, "project"),
  };
}

export function siteApi(ctx, token: string, cloud: string): Api {
  return { ctx, url: ctx.config.mcp_url, token, cloud, site: "", flagField: DEFAULT_FLAG_FIELD };
}

export function raw(api: Api, name: string, args: Record<string, unknown>) {
  return api.ctx.mcp.call({ url: api.url, token: api.token, tool: name, arguments: args });
}

export function tool(api: Api, name: string, args: Record<string, unknown>) {
  return raw(api, name, { ...args, cloudId: api.cloud });
}

export function search(api: Api, jql: string, fields: string[], token?: string) {
  const args: Record<string, unknown> = { jql, fields, maxResults: 100, view: "full", responseContentFormat: "markdown" };
  if (token !== undefined) args.nextPageToken = token;
  return tool(api, "searchJiraIssuesUsingJql", args);
}

export function jqlText(text: string): string {
  return [...text]
    .map((c) => (/[\p{L}\p{N}]/u.test(c) || c === " " ? c : " "))
    .join("")
    .split(/\s+/)
    .filter((word) => word !== "")
    .join(" ");
}

export function jqlKey(key: string): string {
  const valid = key !== "" && /^[A-Za-z0-9_-]+$/.test(key);
  return valid ? key : fail("permanent", `${JSON.stringify(key)} is not a project or issue key`);
}

export function projectOf(api: Api, args: Record<string, unknown>): string {
  const project = optionalText(args, "project") ?? api.project;
  return jqlKey(project ?? blocked(NO_PROJECT));
}

export const browse = (api: Api, key: string) => (api.site === "" ? "" : `${api.site}/browse/${key}`);
