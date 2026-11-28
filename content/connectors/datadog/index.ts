import { type ConfigSchema, type Creds, type Language, entries, fail, pick, settingField, tokenAlternative, tokenField, unknownAction } from "sdk/connectors/index.ts";
import { mcp } from "sdk/connectors/contracts.ts";

export const SITE = "site";

type Ctx = { config: { site: string; sites: string; mcp_url: string } } & Record<string, any>;

export const config: ConfigSchema = {
  mcp_url: { required: true, description: "The MCP server, with {site} for the chosen site" },
  site: { required: true, description: "The site used when none is entered" },
  sites: { required: true, description: "The known sites as site=label, separated by spaces" },
};

export const sites = (ctx: Ctx): string[][] => entries(ctx.config.sites);

export function siteOf(ctx: Ctx, entered: string, fallback: string): string | undefined {
  let bare = entered.trim().replace(/^https:\/\//, "").replace(/^http:\/\//, "");
  bare = (bare.split("/")[0] ?? "").toLowerCase();
  bare = bare.replace(/^app\./, "").replace(/^mcp\./, "");
  if (bare === "") bare = fallback;
  return sites(ctx).map(([site]) => site).find((site) => site === bare);
}

export const serverOf = (ctx: Ctx, site: string) => ctx.config.mcp_url.replace("{site}", site);

export function labelOf(ctx: Ctx, site: string): string {
  const region = sites(ctx).find(([known]) => known === site)?.[1];
  return region !== undefined && site !== ctx.config.site ? `Datadog ${region}` : "Datadog";
}

function unknownSite(ctx: Ctx, language: Language): string {
  const known = sites(ctx).map(([site]) => site).join(", ");
  return pick(language, `Use one of the Datadog sites: ${known}.`, `Usa uno de los sitios de Datadog: ${known}.`, `Use um dos sites do Datadog: ${known}.`);
}

const siteField = (ctx: Ctx, language: Language) =>
  settingField(SITE, pick(language, "Datadog site", "Sitio de Datadog", "Site do Datadog"), "", ctx.config.site);

const chosenSite = (ctx: Ctx, state: { site?: string }) => siteOf(ctx, state?.site ?? "", ctx.config.site) ?? ctx.config.site;

export function provider() {
  return {
    schema_version: 2,
    name: "Datadog",
    logo: "logo.svg",
    capabilities: ["mcp"],
    auth: { kind: "loopback" },
    actions: mcp(),
  };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(language, "Your Datadog account", "Tu cuenta de Datadog", "Sua conta Datadog"),
    group: "mcp",
    button: pick(language, "Continue on Datadog", "Continuar en Datadog", "Continuar no Datadog"),
    site: pick(language, "Datadog", "de Datadog", "do Datadog"),
    choose_title: pick(language, "Datadog site", "Sitio de Datadog", "Site do Datadog"),
    hidden: false,
  };
}

export function alternatives({ language }: { language: Language }, ctx: Ctx) {
  const label = pick(language, "Datadog access token", "Token de acceso de Datadog", "Token de acesso do Datadog");
  const help = pick(
    language,
    "In Datadog, create a personal access token (or a service account token) with the mcp_read scope, and mcp_write if agents may change things.",
    "En Datadog, crea un personal access token (o un token de service account) con el scope mcp_read, y mcp_write si los agentes pueden cambiar cosas.",
    "No Datadog, crie um personal access token (ou um token de service account) com o scope mcp_read, e mcp_write se os agentes puderem alterar algo.",
  );
  const way = pick(language, "Use an access token", "Usar un token de acceso", "Usar token de acesso");
  const site = pick(language, "Datadog", "de Datadog", "do Datadog");
  return [tokenAlternative(language, "access_token", way, site, [siteField(ctx, language), tokenField("token", label, help, true)], "bearer")];
}

export const signIn = {
  name: () => "datadog",
  fields: ({ language }: { language: Language }, ctx: Ctx) => [siteField(ctx, language)],
  forFields({ fields, language }: { fields: Record<string, string>; language: Language }, ctx: Ctx) {
    const site = siteOf(ctx, fields?.[SITE] ?? "", ctx.config.site);
    if (site === undefined) throw new Error(unknownSite(ctx, language));
    return { state: { site } };
  },
  begin: ({ state, port }, ctx: Ctx) => ctx.oauth.mcp.begin({ server: serverOf(ctx, chosenSite(ctx, state)), port }),
  exchange({ state, authorization, code, now }, ctx: Ctx) {
    const server = serverOf(ctx, chosenSite(ctx, state));
    const grant = ctx.oauth.mcp.exchange({ server, authorization, code, now });
    grant.extra = { ...(grant.extra ?? {}), resource: server };
    return grant;
  },
  refresh({ state, grant, now }, ctx: Ctx) {
    const server = grant?.extra?.resource || serverOf(ctx, chosenSite(ctx, state));
    return ctx.oauth.mcp.refresh({ server, grant, now });
  },
};

function siteIn(ctx: Ctx, creds: Creds): string {
  const entered = typeof creds.settings?.[SITE] === "string" ? (creds.settings[SITE] as string) : "";
  return siteOf(ctx, entered, ctx.config.site) ?? fail("permanent", unknownSite(ctx, "en"));
}

export function call({ action, creds }: { action: string; creds: Creds }, ctx: Ctx) {
  if (action !== "mcp.tools") return unknownAction(action);
  const url = typeof creds.settings?.url === "string" && creds.settings.url !== "" ? creds.settings.url : fail("permanent", "the argument `url` is missing or has the wrong type");
  return ctx.mcp.tools({ url, token: creds.token }).map((name: string) => ({ name }));
}

export function accounts({ creds }: { creds: Creds }, ctx: Ctx) {
  const site = siteIn(ctx, creds);
  const url = serverOf(ctx, site);
  const found = ctx.mcp.tools({ url, token: creds.token });
  return [{ id: site, label: labelOf(ctx, site), settings: { url, [SITE]: site, shown: { tools: found.length, host: site } } }];
}
