import { type Creds, type Language, classOf, connect, fail, missing, pick, setting, tokenField, unknownAction } from "sdk/connectors/index.ts";
import { mcp } from "sdk/connectors/contracts.ts";

export const URL_FIELD = "url";
export const LABEL = "label";

type State = { server?: string };

export function enteredUrl(ctx, fields: Record<string, unknown>, language: Language): string {
  const url = typeof fields?.[URL_FIELD] === "string" ? (fields[URL_FIELD] as string).trim() : "";
  if (url === "") {
    throw new Error(pick(language, "Enter the MCP server address.", "Indica la dirección del servidor MCP.", "Informe o endereço do servidor MCP."));
  }
  if (!ctx.net.safeEndpoint({ url })) {
    throw new Error(
      pick(language, "Use an https address, or http only on localhost.", "Usa una dirección https, o http solo en localhost.", "Use um endereço https, ou http só em localhost."),
    );
  }
  return url;
}

const hostOf = (ctx, url: string): string => ctx.net.site({ url }) ?? url;

const serverOf = (state: State): string => (state?.server ? state.server : fail("permanent", "no MCP server was given"));

export function provider() {
  return {
    key: "mcp",
    name: "MCP server",
    logo: "logo.svg",
    capabilities: ["mcp"],
    auth: { kind: "loopback" },
    actions: mcp(),
  };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(
      language,
      "Tools from a remote MCP server for the agents",
      "Herramientas de un servidor MCP remoto para los agentes",
      "Ferramentas de um servidor MCP remoto para os agentes",
    ),
    group: "mcp",
    button: connect(language),
    site: pick(language, "the MCP server", "del servidor MCP", "do servidor MCP"),
    choose_title: pick(language, "MCP server", "Servidor MCP", "Servidor MCP"),
    hidden: true,
  };
}

export const signIn = {
  name: ({ state }: { state: State }) => (state?.server ? state.server : "mcp"),
  fields({ language }: { language: Language }) {
    const address = pick(language, "MCP server address", "Dirección del servidor MCP", "Endereço do servidor MCP");
    const example = pick(language, "For example, https://mcp.example.com/mcp.", "Por ejemplo, https://mcp.ejemplo.com/mcp.", "Por exemplo, https://mcp.exemplo.com/mcp.");
    const name = pick(language, "Name", "Nombre", "Nome");
    const help = pick(
      language,
      "Agents see the tools as mcp__name__tool.",
      "Los agentes ven las herramientas como mcp__nombre__herramienta.",
      "Os agentes veem as ferramentas como mcp__nome__ferramenta.",
    );
    return [tokenField(URL_FIELD, address, example, false), tokenField(LABEL, name, help, false)];
  },
  forFields: ({ fields, language }, ctx) => ({ state: { server: enteredUrl(ctx, fields, language) } }),
  withoutSignIn({ state }: { state: State }, ctx) {
    try {
      ctx.mcp.connect({ url: serverOf(state), token: "" });
      return { access_token: "" };
    } catch (error) {
      if (classOf(error) === "auth") return null;
      throw error;
    }
  },
  begin: ({ state, port }, ctx) => ctx.oauth.mcp.begin({ server: serverOf(state), port }),
  exchange({ state, authorization, code, now }, ctx) {
    const server = serverOf(state);
    const grant = ctx.oauth.mcp.exchange({ server, authorization, code, now });
    grant.extra = { ...(grant.extra ?? {}), resource: server };
    return grant;
  },
  refresh({ state, grant, now }, ctx) {
    const server = grant?.extra?.resource || state?.server || "";
    if (server === "") return fail("auth", "the sign-in lost its MCP server");
    return ctx.oauth.mcp.refresh({ server, grant, now });
  },
};

const urlOf = (creds: Creds): string => setting(creds, URL_FIELD) ?? missing(URL_FIELD);

const tools = (ctx, creds: Creds): string[] => ctx.mcp.tools({ url: urlOf(creds), token: creds.token });

export function call({ action, creds }: { action: string; creds: Creds }, ctx) {
  if (action !== "mcp.tools") return unknownAction(action);
  return tools(ctx, creds).map((name) => ({ name }));
}

export function accounts({ creds }: { creds: Creds }, ctx) {
  const url = urlOf(creds);
  const found = tools(ctx, creds);
  const host = hostOf(ctx, url);
  const label = setting(creds, LABEL) ?? host;
  return [{ id: url, label, settings: { url, shown: { tools: found.length, host } } }];
}
