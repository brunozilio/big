export const spec = {
  name: "ListSessions",
  input: {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  description: `List the user's other {{NAME}} sessions open on this computer: their names and folders. Send one a message with SendMessage (\`to\` its name); its answer reaches you as an event.`,
};

export function execute(input, ctx) {
  return ctx.builtin(input);
}
