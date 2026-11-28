export const spec = {
  name: "Monitor",
  input: {
    "additionalProperties": false,
    "properties": {
      "command": {
        "description": "The command; each line it prints is an event.",
        "type": "string"
      },
      "description": {
        "description": "What it watches, in a few words.",
        "type": "string"
      }
    },
    "required": [
      "command"
    ],
    "type": "object"
  },
  description: `Start a background monitor: a shell command whose every line of output reaches you as an event, while you go on working or when the chat is idle. Use it to follow a log (\`tail -f app.log | grep --line-buffered ERROR\`), a watcher, or a loop that prints when something changes (\`while sleep 30; do gh pr checks 12 | grep -q fail && echo failed; done\`). Print only what matters: every line wakes you. It runs until the command ends or you stop it with KillShell; you are told when it ends. To wait on what a tool returns (an MCP tool, a file), use Watch instead: no shell or credentials needed.`,
  profile: { effects: ["write", "consumes-budget"], risk: "medium", reversible: false, untrusted: true, sensitive: false },
};

type Input = { command: string; description?: string };

export function effects(input: Input | null) {
  return [{ exec: typeof input?.command === "string" ? input.command : "" }, { spawn: "backgroundShell" }];
}

export function summary(input: Input | null) {
  if (typeof input?.description === "string") {
    return input.description;
  }
  return typeof input?.command === "string" ? input.command : "";
}

export function execute(input: Input, ctx) {
  if (typeof input?.command !== "string") {
    return { content: "Missing required parameter `command`.", isError: true };
  }
  let id: string;
  try {
    id = ctx.proc.spawn({ command: input.command }).id;
  } catch (error) {
    return { content: `Could not start the monitor: ${error.message ?? error}`, isError: true };
  }
  ctx.proc.watch({ id, lines: true });
  return `Monitoring as ${id}: each line it prints reaches you as an event. Stop it with KillShell.`;
}
