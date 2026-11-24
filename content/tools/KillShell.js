export const spec = {
  name: "KillShell",
  input: {
    "additionalProperties": false,
    "properties": {
      "shell_id": {
        "description": "The id Bash, Monitor or Watch returned.",
        "type": "string"
      }
    },
    "required": [
      "shell_id"
    ],
    "type": "object"
  },
  description: `Stop a background command (and everything it started), a monitor, or a watch.`,
};

export function execute(input, ctx) {
  return ctx.builtin(input);
}
