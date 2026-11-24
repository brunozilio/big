export const spec = {
  name: "TaskGet",
  input: {
    "additionalProperties": false,
    "properties": {
      "task_id": {
        "type": "string"
      }
    },
    "required": [
      "task_id"
    ],
    "type": "object"
  },
  description: `Read a planning task's description, status and dependencies.`,
};

export function execute(input, ctx) {
  return ctx.builtin(input);
}
