export const spec = {
  name: "StructuredOutput",
  input: (ctx) => ({
    type: "object",
    properties: { result: ctx.schema },
    required: ["result"],
    additionalProperties: false,
  }),
  description: `Deliver the final structured result. All fields must satisfy this schema. Call after doing the requested work and before completing the response. Validation errors must be corrected.`,
  profile: { effects: ["read"], risk: "low", reversible: true, untrusted: false, sensitive: false },
};

type Input = { result?: unknown };

type Checked = { valid: boolean; error?: string };

export function effects(_input: Input | null) {
  return [];
}

export function summary(_input: Input | null) {
  return "Structured result";
}

function failed(content: string) {
  return { content, isError: true };
}

function only(input: unknown) {
  if (input === null || typeof input !== "object" || Array.isArray(input)) {
    return false;
  }
  const keys = Object.keys(input);
  return keys.length === 1 && keys[0] === "result";
}

export function execute(input: Input, ctx) {
  if (!only(input)) {
    return failed("Pass exactly one field named result.");
  }
  const schema = ctx.schema.output();
  if (schema === null || schema === undefined) {
    return failed("This session asks for no structured result.");
  }
  const checked: Checked = ctx.schema.validate({ schema, value: input.result });
  if (!checked.valid) {
    return failed(`Invalid structured result: ${checked.error}`);
  }
  return "Structured result accepted.";
}
