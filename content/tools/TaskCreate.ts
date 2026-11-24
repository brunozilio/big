export const spec = {
  name: "TaskCreate",
  input: {
    "additionalProperties": false,
    "properties": {
      "activeForm": {
        "description": "The subject as an ongoing action, shown while it is in progress, e.g. \"Running the tests\".",
        "type": "string"
      },
      "description": {
        "type": "string"
      },
      "subject": {
        "type": "string"
      }
    },
    "required": [
      "subject"
    ],
    "type": "object"
  },
  description: `Create a task with a subject and description. Returns its task_id. The list belongs to the session and lasts across its turns; in a team, the lead and every teammate share it.`,
};

type Input = { subject?: string; description?: string; activeForm?: string };

export function effects(_input: Input | null) {
  return [];
}

export function summary(input: Input | null) {
  return typeof input?.subject === "string" ? input.subject : "";
}

export function execute(input: Input, ctx) {
  try {
    return JSON.stringify(ctx.tasks.create({ subject: input.subject, description: input.description, activeForm: input.activeForm }));
  } catch (error) {
    return { content: String(error.message ?? error), isError: true };
  }
}
