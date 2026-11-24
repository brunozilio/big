export const spec = {
  name: "AddRepository",
  input: {
    "additionalProperties": false,
    "properties": {
      "directory": {
        "description": "Absolute path of the repository's local checkout.",
        "type": "string"
      },
      "repo": {
        "description": "The remote repository as owner/name, when it has one.",
        "type": "string"
      },
      "run": {
        "description": "The commands that run the application in this repository, one per process.",
        "items": {
          "type": "string"
        },
        "type": "array"
      },
      "verify": {
        "description": "The commands that check a change in this repository (tests, lint, end-to-end tests).",
        "items": {
          "type": "string"
        },
        "type": "array"
      }
    },
    "required": [
      "directory"
    ],
    "type": "object"
  },
  description: `Add a repository to the application this task belongs to, or update the one already registered at that directory. Call it whenever you create a new repository or service for the application, or learn how one is run or verified, so the next steps of this task and later tasks in {{NAME}} work in it too. Fields you leave out keep their current value.`,
};

export function execute(input, ctx) {
  return ctx.builtin(input);
}
