export const spec = {
  name: "AddRepository",
  serial: true,
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
      "kind": {
        "description": "What the repository holds: application (the code of the product), library (code other repositories depend on), configuration (settings and deploy configuration), infrastructure (infrastructure as code: Terraform, CloudFormation, Helm...) or tests (an end-to-end or test suite kept apart).",
        "enum": ["application", "library", "configuration", "infrastructure", "tests"],
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
        "description": "The commands that check a change in this repository (tests, lint).",
        "items": {
          "type": "string"
        },
        "type": "array"
      },
      "e2e": {
        "description": "The commands that run the end-to-end suite (Cypress, Playwright...) that lives in this repository.",
        "items": {
          "type": "string"
        },
        "type": "array"
      },
      "e2eDirectory": {
        "description": "The folder of the end-to-end suite, relative to the repository root. Empty when it is the root.",
        "type": "string"
      },
      "readOnly": {
        "description": "true when the repository must only be consulted (shared configuration, another team's code): later steps read it, but never edit, branch, commit or push in it. Only the person can make a read-only repository writable again.",
        "type": "boolean"
      }
    },
    "required": [
      "directory"
    ],
    "type": "object"
  },
  description: `Add a repository to the application this task belongs to, or update the one already registered at that directory. Call it whenever you create a new repository or service for the application, or learn what it holds, how it is run, verified or how its end-to-end suite runs, so the next steps of this task and later tasks in {{NAME}} work in it too. Fields you leave out keep their current value.`,
  profile: { effects: ["write"], risk: "low", reversible: true, untrusted: false, sensitive: false },
};

type Input = { directory?: string; repo?: string; kind?: string; run?: string[]; verify?: string[]; e2e?: string[]; e2eDirectory?: string; readOnly?: boolean };

export function effects(_input: Input | null) {
  return [{ edits: true }];
}

export function summary(input: Input | null) {
  return typeof input?.directory === "string" ? input.directory.trim() : "";
}

export function execute(input: Input, ctx) {
  try {
    return ctx.hosted.call({ name: spec.name, input });
  } catch (error) {
    return { content: String(error.message ?? error), isError: true };
  }
}
