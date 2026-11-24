export const spec = {
  name: "TaskStop",
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
  description: `Cancel a background agent and wait for its tools to stop.`,
};

export function execute(input, ctx) {
  return ctx.builtin(input);
}
