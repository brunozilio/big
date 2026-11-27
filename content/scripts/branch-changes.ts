export const spec = {
  name: "branch-changes",
  description:
    "Summarize what the current git branch changed against its base: the commits, each changed file with its added and removed lines, and the totals. Uncommitted changes count too.",
  input: {
    type: "object",
    properties: {
      base: {
        type: "string",
        description: "Branch or commit to compare with; when left out, the merge base with main, or else master.",
      },
    },
    additionalProperties: false,
  },
  tools: ["Bash"],
};

type Input = { base?: string };

const EMPTY = "(no output)";

const lines = (text: string): string[] =>
  text.trim() === EMPTY ? [] : text.split("\n").map((line) => line.trimEnd()).filter((line) => line !== "");

async function baseOf(given: string | undefined): Promise<string> {
  if (given) return (await $`git merge-base HEAD ${given}`).trim();
  for (const name of ["main", "master", "origin/main", "origin/master"]) {
    const found = await $`git merge-base HEAD ${name}`.nothrow();
    if (found.exitCode === 0) return found.stdout.trim();
  }
  throw new Error("no base found: pass input.base with the branch or commit to compare with");
}

export async function run(input: Input = {}) {
  const base = await baseOf(input.base);
  const range = `${base}..HEAD`;
  const commits = lines(await $`git log --format=%h%x09%s ${range}`).map((line) => {
    const [hash, ...subject] = line.split("\t");
    return { hash, subject: subject.join("\t") };
  });
  const files = lines(await $`git diff --numstat ${base}`).map((line) => {
    const [added, removed, ...path] = line.split("\t");
    const count = (value: string) => (value === "-" ? null : Number(value));
    return { path: path.join("\t"), added: count(added), removed: count(removed) };
  });
  const sum = (key: "added" | "removed") => files.reduce((total, file) => total + (file[key] ?? 0), 0);
  return { base, commits, files, added: sum("added"), removed: sum("removed") };
}
