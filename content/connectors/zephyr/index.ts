import { type Creds, type Language, optionalText, pick, setting, text, textAt, tokenAlternative, tokenField, unknownAction, useApiToken, words } from "sdk/connectors/index.ts";
import { binding, options, projectOptions, tests } from "sdk/connectors/contracts.ts";
import { items } from "sdk/connectors/mcp.ts";
import { cases, createCase, createCycle, finishCycle, record } from "./cases.ts";
import { type Api, idOf, lastPage, MOST_PAGES, PAGE } from "./api.ts";

export { messages } from "sdk/connectors/index.ts";

export const API_TOKEN = "api_token";

type Ctx = { config: { mcp_url: string; oauth_redirect: string; scopes: string } } & Record<string, any>;

export function provider() {
  return { schema_version: 2, name: "Zephyr", logo: "logo.svg", capabilities: ["tests"], auth: { kind: "loopback" }, actions: [...tests(), options("tests")] };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(language, "Your Zephyr Scale account", "Tu cuenta de Zephyr Scale", "Sua conta Zephyr Scale"),
    group: "tests",
    button: pick(language, "Continue on SmartBear", "Continuar en SmartBear", "Continuar na SmartBear"),
    site: pick(language, "SmartBear", "de SmartBear", "da SmartBear"),
    choose_title: pick(language, "Zephyr account", "Cuenta de Zephyr", "Conta do Zephyr"),
    hidden: false,
    bindings: [binding("tests", "application", "Zephyr", { section: pick(language, "Quality and security", "Calidad y seguridad", "Qualidade e segurança"), order: 60 })],
  };
}

export function alternatives({ language }: { language: Language }) {
  const help = pick(
    language,
    "In Jira, open ⚙ → System settings → Apps → Zephyr API Access Tokens and click Generate access token.",
    "En Jira, abre ⚙ → Configuración del sistema → Apps → Zephyr API Access Tokens y haz clic en Generar token de acceso.",
    "No Jira, abra ⚙ → Configurações gerais → Apps → Zephyr API Access Tokens e clique em Gerar token de acesso.",
  );
  const label = pick(language, "Zephyr API token", "API token de Zephyr", "API token do Zephyr");
  const site = pick(language, "Zephyr", "de Zephyr", "do Zephyr");
  return [tokenAlternative(language, API_TOKEN, useApiToken(language), site, [tokenField("token", label, help, true)], "bearer")];
}

const portOf = (redirect: string) => {
  if (redirect.includes("{port}")) return null;
  const found = /^[a-z]+:\/\/[^/:]+:(\d+)/i.exec(redirect);
  return found ? Number(found[1]) : null;
};

const flow = (ctx: Ctx) => ({ server: ctx.config.mcp_url, redirect: ctx.config.oauth_redirect, scopes: words(ctx.config.scopes) });

export const signIn = {
  name: (_: unknown, ctx: Ctx) => ctx.config.mcp_url,
  port: (_: unknown, ctx: Ctx) => portOf(ctx.config.oauth_redirect),
  begin: ({ port }, ctx: Ctx) => ctx.oauth.mcp.begin({ ...flow(ctx), port }),
  exchange: ({ authorization, code, now }, ctx: Ctx) => ctx.oauth.mcp.exchange({ ...flow(ctx), authorization, code, now }),
  refresh: ({ grant, now }, ctx: Ctx) => ctx.oauth.mcp.refresh({ ...flow(ctx), grant, now }),
};

function apiOf(ctx: Ctx, creds: Creds): Api {
  const url = ctx.config.mcp_url;
  return {
    tool: (name: string, args: unknown) => ctx.mcp.call({ url, token: creds.token, tool: name, arguments: args }),
    project: setting(creds, "project"),
    ctx,
  };
}

function projectOf(project: unknown) {
  const key = textAt(project, ["key"]);
  const name = textAt(project, ["name"]);
  return { id: idOf(project, ["id"]), name: name === "" ? key : name, key };
}

function allProjects(api: Api) {
  const found: { id: string; name: string; key: string }[] = [];
  for (let page = 0; page < MOST_PAGES; page++) {
    const answer = api.tool("zephyr_get_projects", { startAt: page * PAGE, maxResults: PAGE });
    const projects = items(answer, "values");
    found.push(...projects.map(projectOf).filter((project) => project.key !== ""));
    if (lastPage(answer, projects.length)) break;
  }
  return found;
}

function projects(api: Api, args: Record<string, unknown>) {
  const query = (optionalText(args, "query") ?? "").trim().toLowerCase();
  return allProjects(api).filter(
    (project) => query === "" || project.key.toLowerCase().includes(query) || project.name.toLowerCase().includes(query),
  );
}

export function call(
  { action, args, creds, idempotencyKey }: { action: string; args: Record<string, unknown>; creds: Creds; idempotencyKey: string },
  ctx: Ctx,
) {
  const api = apiOf(ctx, creds);
  if (action === "tests.create_case") return createCase(api, args, idempotencyKey);
  if (action === "tests.cases") return cases(api, text(args, "issue"));
  if (action === "tests.create_cycle") return createCycle(api, args, idempotencyKey);
  if (action === "tests.record") return record(api, args, idempotencyKey);
  if (action === "tests.finish_cycle") return finishCycle(api, args);
  if (action === "tests.projects") return projects(api, args);
  if (action === "tests.options") return projectOptions(projects(api, args));
  return unknownAction(action);
}

export function accounts({ creds }: { creds: Creds }, ctx: Ctx) {
  const found = allProjects(apiOf(ctx, creds));
  const keys = found.map((project) => project.key);
  const settings: Record<string, unknown> = { shown: { projects: keys } };
  if (keys.length === 1) settings.project = keys[0];
  return [{ id: "zephyr", label: "Zephyr Scale", settings }];
}
