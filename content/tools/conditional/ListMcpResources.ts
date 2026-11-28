const SHOWN = 60000;

export const spec = {
  name: "ListMcpResources",
  deferred: true,
  input: {
    type: "object",
    properties: { server: { type: "string", description: "The server whose resources to list." } },
    required: ["server"],
    additionalProperties: false,
  },
  description: (ctx) =>
    `List the resources (files, records, documents) MCP servers offer: ${ctx.servers.join(", ")}. Read one with ReadMcpResource.`,
  profile: { effects: ["read"], risk: "low", reversible: true, untrusted: false, sensitive: false },
};

type Input = { server?: string };

type Resource = { uri?: unknown; name?: unknown; description?: unknown };

type Listed = { server: string; resources?: Resource[]; error?: string };

export function effects(input: Input | null) {
  const server = typeof input?.server === "string" ? input.server : "";
  return [{ resource: { server, uri: "" } }];
}

export function summary(input: Input | null) {
  return typeof input?.server === "string" ? input.server : "";
}

function failed(content: string) {
  return { content, isError: true };
}

function clip(text: string, limit: number) {
  const chars = Array.from(text);
  if (chars.length <= limit) {
    return text;
  }
  return `${chars.slice(0, limit).join("")}\n… [${chars.length - limit} more characters cut]`;
}

function line(server: string, resource: Resource) {
  const uri = typeof resource?.uri === "string" ? resource.uri : "";
  const name = typeof resource?.name === "string" ? resource.name : uri;
  const about = typeof resource?.description === "string" ? resource.description : "";
  return `${server}: ${name} <${uri}> ${about}\n`;
}

export function execute(input: Input, ctx) {
  if (typeof input?.server !== "string") {
    return failed("Missing required parameter `server`.");
  }
  let listed: Listed[];
  try {
    listed = ctx.mcp.resources({ server: input.server });
  } catch (error) {
    return failed(String(error.message ?? error));
  }
  let out = "";
  for (const found of listed) {
    if (typeof found.error === "string") {
      out += `${found.server}: could not list: ${found.error}\n`;
      continue;
    }
    for (const resource of Array.isArray(found.resources) ? found.resources : []) {
      out += line(found.server, resource);
    }
  }
  return out === "" ? "No resources." : clip(out.trimEnd(), SHOWN);
}
