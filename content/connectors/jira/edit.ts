import { isObject, missing, optionalText, text, textAt } from "sdk/connectors/index.ts";
import { type Api, jqlKey, tool } from "./api.ts";
import { read, summary } from "./tracker.ts";

const DEFAULT_LINK = "Relates";

type Args = Record<string, any>;

export function updateIssue(api: Api, args: Args) {
  const issue = jqlKey(text(args, "issue"));
  const fields: Args = isObject(args.fields) ? { ...args.fields } : {};
  const title = optionalText(args, "title");
  if (title !== undefined) fields.summary = title;
  if (typeof args.description === "string") fields.description = args.description;
  if (Object.keys(fields).length === 0) missing("fields");
  const answer = tool(api, "editJiraIssue", { issueIdOrKey: issue, fields, contentFormat: "markdown" });
  return { ...summary(api, answer), ref: issue, changed: true };
}

function linked(links, other: string): boolean {
  const lower = other.toLowerCase();
  return (Array.isArray(links) ? links : []).some(
    (link) => textAt(link, "/inwardIssue/key").toLowerCase() === lower || textAt(link, "/outwardIssue/key").toLowerCase() === lower,
  );
}

export function linkIssues(api: Api, args: Args) {
  const issue = jqlKey(text(args, "issue"));
  const to = jqlKey(text(args, "to"));
  const current = read(api, issue, ["issuelinks"]);
  if (linked(current?.fields?.issuelinks, to)) return { issue, to, already: true };
  const kind = optionalText(args, "kind") ?? DEFAULT_LINK;
  tool(api, "createIssueLink", { inwardIssue: to, outwardIssue: issue, type: kind });
  return { issue, to, already: false };
}
