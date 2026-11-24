export const spec = {
  name: "TaskUpdate",
  input: {
    "additionalProperties": false,
    "properties": {
      "blocked_by": {
        "items": {
          "type": "string"
        },
        "type": "array"
      },
      "description": {
        "type": "string"
      },
      "owner": {
        "description": "Who works on it: your own name to claim it, a teammate's name to give it to them (they are told), or empty to hand it back.",
        "type": "string"
      },
      "status": {
        "enum": [
          "pending",
          "in_progress",
          "completed"
        ]
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
  description: `Update a task. Status is pending, in_progress or completed; a task cannot start or end while a task in its blocked_by is open. blocked_by lists the task_ids that must be completed first (each task shows the ones it blocks). owner claims a task: a task someone else holds cannot be claimed, except by the lead, and a teammate that stops hands its open tasks back.`,
};

const STATUSES = ["pending", "in_progress", "completed"];

type Input = {
  task_id?: string;
  status?: unknown;
  description?: unknown;
  blocked_by?: unknown;
  addBlockedBy?: unknown;
  owner?: unknown;
};

type Change = { task_id: string; status?: string; description?: string; blocked_by?: string[]; owner?: string };

export function effects(_input: Input | null) {
  return [];
}

export function summary(input: Input | null) {
  return typeof input?.task_id === "string" ? input.task_id : "";
}

function blockers(input: Input): string[] | undefined {
  const listed = "blocked_by" in input ? input.blocked_by : input.addBlockedBy;
  if (!("blocked_by" in input) && !("addBlockedBy" in input)) {
    return undefined;
  }
  if (!Array.isArray(listed) || !listed.every((id) => typeof id === "string")) {
    throw new Error("blocked_by must be an array of task ids");
  }
  return listed;
}

function change(input: Input): Change {
  const asked: Change = { task_id: typeof input.task_id === "string" ? input.task_id : "" };
  if (typeof input.status === "string") {
    if (!STATUSES.includes(input.status)) {
      throw new Error("status is pending, in_progress or completed");
    }
    asked.status = input.status;
  }
  const blocked = blockers(input);
  if (blocked !== undefined) {
    asked.blocked_by = blocked;
  }
  if ("description" in input) {
    if (typeof input.description !== "string") {
      throw new Error("description must be text");
    }
    asked.description = input.description;
  }
  if (typeof input.owner === "string") {
    asked.owner = input.owner;
  }
  return asked;
}

export function execute(input: Input, ctx) {
  try {
    return JSON.stringify(ctx.tasks.update(change(input)));
  } catch (error) {
    return { content: String(error.message ?? error), isError: true };
  }
}
