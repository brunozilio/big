const FOUND = 5;
const MIN_WORD = 3;
const NAME_WEIGHT = 3;

type Script = { name: string; description: string; input: unknown; tools: string[]; missing: string[] };

const words = (text: string): string[] =>
  text
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter((word) => [...word].length >= MIN_WORD);

function score(script: Script, wanted: string[]): number {
  const name = script.name.toLowerCase();
  const description = script.description.toLowerCase();
  return wanted.reduce((sum, word) => sum + (name.includes(word) ? NAME_WEIGHT : description.includes(word) ? 1 : 0), 0);
}

const SYSTEM = `You adapt a script from a bank of TypeScript scripts that an AI agent runs inside its REPL. The script almost fits what the agent needs now; change it so that it does exactly the goal, and change nothing else.

The script's shape stays the same: \`export const spec = { name, description, input, tools }\` (input is a JSON schema of what run takes, tools names every tool the script calls) and \`export async function run(input)\`, returning plain data. It has no imports. It calls the agent's tools as global async functions taking the tool's input object (e.g. \`await Grep({ pattern, output_mode: "count" })\`), and the helpers \`file(path).text()\`, \`write(path, text)\`, \`edit(path, oldText, newText)\`, the shell tag $\`git status\` (throws on failure; .nothrow() gives { stdout, exitCode }), \`sleep(ms)\` and \`scripts["name"](input)\` for other scripts of the bank. Use only the tools listed as available. Update spec.description, spec.input and spec.tools when what the script does, takes or calls changes, and give spec.name a new name only when the goal makes it a different script.

Answer with the whole adapted script in one \`\`\`ts block, then at most three short lines on what you changed.`;

export function adapting({ source, goal, input, tools }: { source: string; goal: string; input: unknown; tools: string[] }) {
  const prompt = [
    `<script>\n${source}\n</script>`,
    `<goal>\n${goal}\n</goal>`,
    `<input>\n${JSON.stringify(input ?? {}, null, 2)}\n</input>`,
    `<available_tools>\n${(tools ?? []).join(", ")}\n</available_tools>`,
  ].join("\n\n");
  return { system: SYSTEM, prompt };
}

export function adapted({ answer }: { answer: string }) {
  const text = String(answer ?? "");
  const fenced = /```(?:ts|typescript|js|javascript)?[ \t]*\n([\s\S]*?)```/.exec(text);
  const source = (fenced ? fenced[1] : text).trim();
  const notes = fenced ? text.replace(fenced[0], "").trim() : "";
  return { source: `${source}\n`, notes };
}

export function search({ scripts, query }: { scripts: Script[]; query: string }) {
  const asked = String(query ?? "").trim();
  if (asked.startsWith("select:")) {
    const names = asked.slice("select:".length).split(",").map((name) => name.trim());
    return scripts.filter((script) => names.includes(script.name));
  }
  const wanted = words(asked);
  return scripts
    .map((script, at) => ({ script, at, score: score(script, wanted) }))
    .filter((found) => found.score > 0)
    .sort((a, b) => b.score - a.score || a.at - b.at)
    .slice(0, FOUND)
    .map((found) => found.script);
}
