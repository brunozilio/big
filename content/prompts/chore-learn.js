export default `[LEARNING MODE: propose one lasting lesson from this conversation.]

What the user corrected or redirected, or what the agent did that worked, is listed above. Decide whether that points to something the agent should do differently, or keep ready, in every future session of this project, and if so propose exactly one of:
- "memory": a fact or a way of working to remember (for example "This project uses pnpm, never npm").
- "skill": a repeatable procedure worth writing down as a skill; \`name\` is a short lowercase-hyphenated skill name and \`text\` says what the skill should do, step by step.
- "instruction": one short line for the project's instructions file, when the user stated a rule for how all work in this project is done (for example "Run cargo nextest, never cargo test"); \`text\` is that line and \`name\` is empty. Prefer "memory" for facts, and never put keys, tokens or passwords in it.
- "script": when REPL scripts that worked in this conversation make a flow the agent is likely to run again, a script for the bank that a person approves before agents use it. \`name\` is a short lowercase-hyphenated script name and \`text\` is the whole TypeScript file, built only from the code of the REPL scripts that worked (never from ones that failed), with what varied turned into input: \`export const spec = { name, description, input, tools }\` (\`description\` says what it does so a search finds it, \`input\` is a JSON schema of what \`run\` takes, \`tools\` names every tool it calls) and \`export async function run(input)\` returning plain data. It has no imports and calls the tools as the REPL scripts did (\`await Grep({ ... })\`, \`file(path).text()\`, the shell tag \`$\`). Never put keys, tokens, passwords or one-off paths in it.

Propose only what the user's own words or actions, or the scripts that worked, support; never invent a preference. If nothing lasting was learned, answer with only the word none.

Otherwise answer with only a JSON object, no prose:
{"kind": "memory" | "skill" | "instruction" | "script", "name": string, "text": string, "why": string}
\`why\` quotes or paraphrases what the user said or did, in one sentence.`;
