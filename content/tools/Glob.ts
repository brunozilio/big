const LIMIT = 100;

export const spec = {
  name: "Glob",
  input: {
    additionalProperties: false,
    properties: {
      path: { description: "The directory to search; the working directory if omitted.", type: "string" },
      pattern: { description: "The glob to match paths against.", type: "string" },
    },
    required: ["pattern"],
    type: "object",
  },
  description: `Find files by name with a glob pattern such as \`**/*.rs\` or \`src/**/mod.rs\`, searching the working directory or \`path\`. Files ignored by git are skipped. Returns up to ${LIMIT} paths, most recently modified first. Use Grep to search file contents.`,
  profile: { effects: ["read"], risk: "low", reversible: true, untrusted: false, sensitive: false },
};

type Input = { pattern: string; path?: string };

export function effects(input: Input | null) {
  return [{ readTree: input?.path ?? "." }];
}

export function summary(input: Input | null) {
  return input?.pattern ?? "";
}

export function execute(input: Input, ctx) {
  let found: { path: string; modified: number }[];
  try {
    found = ctx.fs.walk({ root: input.path ?? ".", glob: input.pattern });
  } catch (error) {
    return { content: String(error.message ?? error), isError: true };
  }
  if (found.length === 0) {
    return "No files found.";
  }
  found.sort((a, b) => b.modified - a.modified || (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  const shown = found.slice(0, LIMIT).map((file) => file.path);
  if (found.length > LIMIT) {
    shown.push(`… and ${found.length - LIMIT} more; narrow the pattern.`);
  }
  return shown.join("\n");
}
