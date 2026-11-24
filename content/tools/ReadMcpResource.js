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

export function execute(input, ctx) {
  return ctx.builtin(input);
}
