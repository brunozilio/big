const SHOWN = 60000;

export const spec = {
  name: "ReadMcpResource",
  input: {
    type: "object",
    properties: { server: { type: "string" }, uri: { type: "string" } },
    required: ["server", "uri"],
    additionalProperties: false,
  },
  description: `Read a resource of an MCP server by its URI (ListMcpResources lists them).`,
};

type Input = { server?: string; uri?: string };

type Content = { text?: unknown; blob?: unknown; mimeType?: unknown };

export function effects(input: Input | null) {
  const server = typeof input?.server === "string" ? input.server : "";
  const uri = typeof input?.uri === "string" ? input.uri : "";
  return [{ resource: { server, uri } }];
}

export function summary(input: Input | null) {
  return typeof input?.uri === "string" ? input.uri : "";
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

function shown(content: Content) {
  if (typeof content?.text === "string") {
    return content.text;
  }
  const size = typeof content?.blob === "string" ? content.blob.length : 0;
  const kind = typeof content?.mimeType === "string" ? content.mimeType : "data";
  return `[${size} bytes of ${kind}]`;
}

export function execute(input: Input, ctx) {
  if (typeof input?.server !== "string") {
    return failed("Missing required parameter `server`.");
  }
  if (typeof input.uri !== "string") {
    return failed("Missing required parameter `uri`.");
  }
  let read;
  try {
    read = ctx.mcp.read({ server: input.server, uri: input.uri });
  } catch (error) {
    return failed(String(error.message ?? error));
  }
  const contents: Content[] = Array.isArray(read?.contents) ? read.contents : [];
  return clip(contents.map(shown).join("\n\n"), SHOWN);
}
