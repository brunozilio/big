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

type Input = { task_id?: string };

export function effects(_input: Input | null) {
  return [{ cancel: { own: true } }];
}

export function summary(input: Input | null) {
  return typeof input?.task_id === "string" ? input.task_id : "";
}

export function execute(input: Input, ctx) {
  if (!ctx.background.stop({ task_id: input.task_id })) {
    return { content: "Unknown task_id for this run.", isError: true };
  }
  return "Task stopped and joined.";
}
