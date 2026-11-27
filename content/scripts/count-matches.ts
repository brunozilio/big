export const spec = {
  name: "count-matches",
  description:
    "Count the lines matching a regular expression (TODO markers, a deprecated call, a feature flag) in each folder and file, largest first, without reading the matches.",
  input: {
    type: "object",
    properties: {
      pattern: { type: "string", description: "Regular expression to count, as Grep takes it." },
      path: { type: "string", description: "Folder to search in; the project when left out." },
      glob: { type: "string", description: "Only files whose names match this glob, e.g. \"*.rs\"." },
      depth: { type: "integer", description: "How many folder levels to group the counts by (1 when left out)." },
      top: { type: "integer", description: "How many files and folders to list (20 when left out)." },
    },
    required: ["pattern"],
    additionalProperties: false,
  },
  tools: ["Grep"],
};

type Input = { pattern: string; path?: string; glob?: string; depth?: number; top?: number };

const largest = (counts: Map<string, number>, top: number) =>
  [...counts.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, top)
    .map(([path, count]) => ({ path, count }));

function folderOf(file: string, root: string, depth: number): string {
  const inside = root !== "" && file.startsWith(`${root}/`) ? file.slice(root.length + 1) : file.replace(/^\.\//, "");
  const parts = inside.split("/").slice(0, -1);
  return parts.length === 0 ? "." : parts.slice(0, depth).join("/");
}

export async function run(input: Input) {
  if (typeof input?.pattern !== "string" || input.pattern === "") throw new Error("input.pattern is required");
  const found = await Grep({ pattern: input.pattern, path: input.path, glob: input.glob, output_mode: "count" });
  if (found?.error) throw new Error(String(found.error));
  const depth = Math.max(1, input.depth ?? 1);
  const top = Math.max(1, input.top ?? 20);
  const root = (input.path ?? "").replace(/\/+$/, "");
  const files = new Map<string, number>(Object.entries(found.counts ?? {}));
  const folders = new Map<string, number>();
  for (const [file, count] of files) {
    const folder = folderOf(file, root, depth);
    folders.set(folder, (folders.get(folder) ?? 0) + count);
  }
  const total = [...files.values()].reduce((sum, count) => sum + count, 0);
  return { total, files: found.numFiles ?? files.size, folders: largest(folders, top), largest: largest(files, top), more: found.more ?? 0 };
}
