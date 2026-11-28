export const spec = {
  name: "KillShell",
  input: {
    additionalProperties: false,
    properties: {
      shell_id: {
        description: "The id Bash, Monitor or Watch returned.",
        type: "string",
      },
    },
    required: ["shell_id"],
    type: "object",
  },
  description: `Stop a background command (and everything it started), a monitor, or a watch.`,
  profile: { effects: ["write"], risk: "medium", reversible: false, untrusted: false, sensitive: false },
};

type Input = { shell_id?: string };

export function effects(input: Input | null) {
  return [{ cancel: { shell: input?.shell_id ?? "" } }];
}

export function summary(input: Input | null) {
  return input?.shell_id ?? "";
}

export function execute(input: Input, ctx: any) {
  const id = input.shell_id;
  if (typeof id !== "string") {
    return { content: "Missing required parameter `shell_id`.", isError: true };
  }
  const killed = ctx.proc.kill({ id });
  if (!killed) {
    return { content: `No background command \`${id}\`.`, isError: true };
  }
  return { content: `Stopped ${id} (${killed.stopped}).`, background: { Ended: id } };
}
