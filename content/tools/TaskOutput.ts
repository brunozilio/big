export const spec = {
  name: "TaskOutput",
  input: {
    "additionalProperties": false,
    "properties": {
      "block": {
        "type": "boolean"
      },
      "task_id": {
        "type": "string"
      }
    },
    "required": [
      "task_id"
    ],
    "type": "object"
  },
  description: `Collect a background agent's report. block defaults to true; false polls without waiting.`,
};

type Input = { task_id?: string; block?: boolean };

type Found = null | { done: false } | { done: true; report: { content: string; isError: boolean } };

export function effects(_input: Input | null) {
  return [];
}

export function summary(input: Input | null) {
  return typeof input?.task_id === "string" ? input.task_id : "";
}

export function execute(input: Input, ctx) {
  const block = typeof input.block === "boolean" ? input.block : true;
  const found: Found = ctx.background.output({ task_id: input.task_id, block });
  if (found === null) {
    return { content: "Unknown task_id for this run.", isError: true };
  }
  return found.done ? found.report : "Task is still running.";
}
