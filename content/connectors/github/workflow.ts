export type WorkflowJob = { key: string; name: string | null; needs: string[] };

const indent = (line: string) => line.length - line.trimStart().length;

function meaningful(line: string): boolean {
  const trimmed = line.trim();
  return trimmed !== "" && !trimmed.startsWith("#");
}

function unquoted(text: string): string {
  const kept = (text.split(" #")[0] ?? "").trim();
  return kept.replace(/^["']+|["']+$/g, "");
}

function inlineList(value: string): string[] {
  const trimmed = value.trim();
  if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
    return trimmed.slice(1, -1).split(",").map(unquoted).filter((item) => item !== "");
  }
  return trimmed === "" ? [] : [unquoted(trimmed)];
}

function jobsBlock(yaml: string): string[] {
  const lines = yaml.split(/\r?\n/).filter(meaningful);
  const start = lines.findIndex((line) => indent(line) === 0 && line.trimEnd() === "jobs:");
  if (start < 0) return [];
  const block: string[] = [];
  for (const line of lines.slice(start + 1)) {
    if (indent(line) === 0) break;
    block.push(line);
  }
  return block;
}

export function jobs(yaml: string): WorkflowJob[] {
  const block = jobsBlock(yaml);
  if (block.length === 0) return [];
  const jobIndent = indent(block[0]);
  const found: WorkflowJob[] = [];
  let propertyIndent: number | undefined;
  let inNeeds = false;
  for (const line of block) {
    const depth = indent(line);
    const trimmed = line.trim();
    if (depth === jobIndent) {
      found.push({ key: unquoted(trimmed.replace(/:+$/, "")), name: null, needs: [] });
      propertyIndent = undefined;
      inNeeds = false;
      continue;
    }
    const job = found[found.length - 1];
    if (!job) continue;
    if (propertyIndent === undefined) propertyIndent = depth;
    if (depth === propertyIndent) {
      inNeeds = false;
      if (trimmed.startsWith("name:")) {
        const name = unquoted(trimmed.slice(5));
        job.name = name === "" ? null : name;
      } else if (trimmed.startsWith("needs:")) {
        job.needs = inlineList(trimmed.slice(6));
        inNeeds = job.needs.length === 0;
      }
    } else if (inNeeds && trimmed.startsWith("- ")) {
      job.needs.push(unquoted(trimmed.slice(2)));
    }
  }
  return found;
}

export function matches(job: WorkflowJob, shown: string): boolean {
  const first = shown.split(" / ")[0] ?? shown;
  const base = first.split(" (")[0] ?? first;
  const named = job.name !== null && !job.name.includes("${{") ? job.name : null;
  return base === job.key || (named !== null && (named === base || named === shown));
}
