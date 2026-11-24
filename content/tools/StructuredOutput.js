export const spec = {
  name: "StructuredOutput",
  input: (ctx) => ({
    type: "object",
    properties: { result: ctx.schema },
    required: ["result"],
    additionalProperties: false,
  }),
  description: `Deliver the final structured result. All fields must satisfy this schema. Call after doing the requested work and before completing the response. Validation errors must be corrected.`,
};

export function execute(input, ctx) {
  return ctx.builtin(input);
}
