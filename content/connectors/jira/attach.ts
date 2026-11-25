import { ActionError, authorized, checked, fail, json, listOf, send, text } from "sdk/connectors/index.ts";
import { type Api, jqlKey } from "./api.ts";
import { issue } from "./tracker.ts";

const REST_GATEWAY = "https://api.atlassian.com/ex/jira";

const NO_UPLOAD =
  "Jira refused the upload. Attachments go through the Jira REST API: connect Jira with an API token (the sign-in of the Atlassian MCP server does not allow uploads).";

type Wanted = { name: string; contentType: string; base64: string };

function restBase(api: Api): string {
  if (api.token.startsWith("Basic ")) return api.site !== "" ? api.site : fail("permanent", "the Jira site is unknown; reconnect Jira");
  return `${REST_GATEWAY}/${api.cloud}`;
}

function wantedOf(file: any): Wanted | string {
  const name = typeof file?.name === "string" ? file.name : "";
  if (name === "") return "a file without a name";
  if (typeof file?.base64 !== "string" || file.base64 === "") return `${name}: ${file?.skipped ?? "not found"}`;
  return { name, contentType: typeof file?.contentType === "string" ? file.contentType : "application/octet-stream", base64: file.base64 };
}

function upload(api: Api, key: string, file: Wanted) {
  const headers = { ...authorized(api.token), accept: "application/json", "x-atlassian-token": "no-check" };
  const part = { name: "file", filename: file.name, contentType: file.contentType, base64: file.base64 };
  const answer = send(api.ctx, { method: "POST", url: `${restBase(api)}/rest/api/3/issue/${key}/attachments`, headers, parts: [part] });
  if (answer.status === 401 || answer.status === 403) throw new ActionError("permanent", NO_UPLOAD);
  return json(checked(answer));
}

export function attach(api: Api, args: Record<string, unknown>) {
  const key = jqlKey(text(args, "issue"));
  const present = new Set(issue(api, key).attachments.map((found) => found.name));
  const attached: string[] = [];
  const skipped: string[] = [];
  for (const file of listOf(args, "files")) {
    const wanted = wantedOf(file);
    if (typeof wanted === "string") skipped.push(wanted);
    else if (present.has(wanted.name)) attached.push(wanted.name);
    else {
      upload(api, key, wanted);
      present.add(wanted.name);
      attached.push(wanted.name);
    }
  }
  return { attached, skipped };
}
