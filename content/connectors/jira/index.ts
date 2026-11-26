import { type Creds, type Language, optionalText, pick, text, textAt, tokenAlternative, tokenField, unknownAction, words } from "sdk/connectors/index.ts";
import { binding, options, projectOptions, tracker } from "sdk/connectors/contracts.ts";
import { items } from "sdk/connectors/mcp.ts";
import { NO_SITES, apiOf, projectOf, siteApi } from "./api.ts";
import { attach } from "./attach.ts";
import { linkIssues, updateIssue } from "./edit.ts";
import { fields, statuses } from "./metadata.ts";
import { schema } from "./schema.ts";
import { visibleProjects } from "./projects.ts";
import { search } from "./search.ts";
import { comment, createIssue, issue, projects, setImpediment, transition, transitions } from "./tracker.ts";

export { poll } from "./events.ts";

export function provider() {
  return {
    name: "Jira",
    logo: "logo.svg",
    capabilities: ["tracker"],
    auth: { kind: "loopback" },
    actions: [...tracker(), options("tracker")],
    chooses: ["cloud_id"],
  };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(language, "Your Atlassian account", "Tu cuenta de Atlassian", "Sua conta Atlassian"),
    group: "tracker",
    button: pick(language, "Continue on Atlassian", "Continuar en Atlassian", "Continuar na Atlassian"),
    site: pick(language, "Atlassian", "de Atlassian", "da Atlassian"),
    choose_title: pick(language, "Jira site", "Sitio de Jira", "Site do Jira"),
    hidden: false,
    bindings: [binding("tracker", "application", "Jira", { multiple: true })],
  };
}

export function alternatives({ language }: { language: Language }) {
  const email = tokenField("email", pick(language, "Atlassian account email", "Correo de la cuenta de Atlassian", "E-mail da conta Atlassian"), "", false);
  const help = pick(
    language,
    "At id.atlassian.com, open Security and create an API token. The organization admin must allow API tokens on the Rovo MCP server.",
    "En id.atlassian.com, abre Seguridad y crea un API token. El admin de la organización debe permitir API tokens en el Rovo MCP server.",
    "Em id.atlassian.com, abra Segurança e crie um API token. O admin da organização precisa permitir API token no Rovo MCP server.",
  );
  const token = tokenField("token", pick(language, "Atlassian API token", "API token de Atlassian", "API token da Atlassian"), help, true);
  const label = pick(language, "Use an API token", "Usar un API token", "Usar API token");
  const site = pick(language, "Atlassian", "de Atlassian", "da Atlassian");
  return [tokenAlternative(language, "api_token", label, site, [email, token], "basic")];
}

export const signIn = {
  name: (_, ctx) => ctx.config.mcp_url,
  begin: ({ port }, ctx) => ctx.oauth.mcp.begin({ server: ctx.config.mcp_url, port, scopes: words(ctx.config.scopes) }),
  exchange: ({ authorization, code, now }, ctx) => ctx.oauth.mcp.exchange({ server: ctx.config.mcp_url, authorization, code, now, scopes: words(ctx.config.scopes) }),
  refresh: ({ grant, now }, ctx) => ctx.oauth.mcp.refresh({ server: ctx.config.mcp_url, grant, now, scopes: words(ctx.config.scopes) }),
};

type Input = { action: string; args: Record<string, any>; creds: Creds; idempotencyKey: string };

export function call({ action, args, creds, idempotencyKey }: Input, ctx) {
  const api = apiOf(ctx, creds);
  switch (action) {
    case "tracker.create_issue":
      return createIssue(api, args, idempotencyKey);
    case "tracker.issue":
      return issue(api, text(args, "issue"));
    case "tracker.comment":
      return comment(api, text(args, "issue"), text(args, "body"), idempotencyKey);
    case "tracker.statuses":
      return statuses(api, projectOf(api, args), transitions);
    case "tracker.fields":
      return fields(api, args);
    case "tracker.transition":
      return transition(api, text(args, "issue"), { status: optionalText(args, "status"), category: optionalText(args, "category") });
    case "tracker.schema":
      return schema(api, projectOf(api, args));
    case "tracker.set_impediment":
      return setImpediment(api, args, idempotencyKey);
    case "tracker.search":
      return search(api, args);
    case "tracker.projects":
      return projects(api, args);
    case "tracker.options":
      return projectOptions(projects(api, args));
    case "tracker.update_issue":
      return updateIssue(api, args);
    case "tracker.link_issues":
      return linkIssues(api, args);
    case "tracker.attach":
      return attach(api, args);
    default:
      return unknownAction(action);
  }
}

function siteId(site): string {
  const id = textAt(site, "/cloudId");
  return id === "" ? textAt(site, "/id") : id;
}

function siteLabel(site): string {
  const label = textAt(site, "/url").replace(/^(https:\/\/)+/, "").replace(/\/+$/, "");
  return label === "" ? textAt(site, "/name") : label;
}

function siteSettings(site, found: { key: string }[]) {
  const keys = found.map((project) => project.key);
  const settings: Record<string, unknown> = { cloud_id: siteId(site), site_url: textAt(site, "/url") };
  if (keys.length === 0) settings.detail = textAt(site, "/name");
  else settings.shown = { projects: keys };
  if (keys.length === 1) settings.project = keys[0];
  return settings;
}

function projectsOf(ctx, token: string, id: string) {
  try {
    return visibleProjects(siteApi(ctx, token, id));
  } catch (_) {
    return [];
  }
}

export function accounts({ creds }: { creds: Creds }, ctx) {
  const answer = ctx.mcp.call({ url: ctx.config.mcp_url, token: creds.token, tool: "getAccessibleAtlassianResources", arguments: {} });
  const sites = items(answer, "resources").filter((site) => siteId(site) !== "");
  if (sites.length === 0) throw new Error(NO_SITES);
  return sites.map((site) => {
    const id = siteId(site);
    return { id, label: siteLabel(site), settings: siteSettings(site, projectsOf(ctx, creds.token, id)) };
  });
}
