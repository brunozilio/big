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
  profile: { effects: ["read"], risk: "low", reversible: true, untrusted: false, sensitive: false },
};

type Input = { task_id?: string };

export function effects(_input: Input | null) {
  return [];
}

export function summary(input: Input | null) {
  return typeof input?.task_id === "string" ? input.task_id : "";
}

export function execute(input: Input, ctx) {
  try {
    return JSON.stringify(ctx.tasks.get(typeof input.task_id === "string" ? input.task_id : ""));
  } catch (error) {
    return { content: String(error.message ?? error), isError: true };
  }
}
