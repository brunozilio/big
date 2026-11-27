export const spec = {
  name: "REPL",
  serial: true,
  input: {
    "additionalProperties": false,
    "properties": {
      "code": {
        "description": "JavaScript to run. Top-level await works, and what it declares stays for the next call.",
        "type": "string"
      },
      "description": {
        "description": "What the script does, in a few words of active voice, e.g. \"Count TODOs in each crate\".",
        "type": "string"
      },
      "timeout": {
        "description": "Stop the script once its own JavaScript has run this many milliseconds without a break; time spent waiting on tool calls does not count (30000 by default, at most 600000).",
        "type": "integer"
      }
    },
    "required": [
      "code"
    ],
    "type": "object"
  },
  description: `Run JavaScript that calls your other tools as functions, so that loops, filters and chains of calls happen in code instead of in turns. Each of your other tools is a global async function taking that tool's input object, e.g. \`await Grep({ pattern: "TODO", output_mode: "count" })\`; a name that is not an identifier is reached as \`tools["name"]\`. A call resolves to the tool's text output, except: the task tools (TaskCreate, TaskGet, TaskList, TaskUpdate) resolve to the task objects; Glob and Grep listing files resolve to \`{ filenames, numFiles, more }\`, and Grep with \`output_mode: "count"\` to \`{ counts, numFiles, more }\` (\`more\` is how many were left out). Grep with \`output_mode: "content"\` stays text. Every other result is a plain string, not JSON: Bash resolves to what the command printed (with \`run_in_background\`, a sentence such as \`Started in the background as bash_1. ...\`), Read to the file's lines with their numbers, Edit and Write to a sentence saying what changed. \`JSON.parse\` a result only when the command itself printed JSON (\`gh ... --json\`, \`cat x.json\`), and use \`file(path).json()\` for a JSON file. Images a call returns (a Read of an image, a screenshot from an MCP server) come back to you beside the script's output, up to 8. When a built-in tool fails or is refused, the call resolves to \`{ error: "why" }\` instead; an MCP tool's call throws a \`ToolError\` (\`.tool\` names it). Check for \`error\` and catch only where the script can go on without that result, and never treat a failure as success.

Helpers for the common cases, each going through the matching tool (and its permission checks and hooks) and throwing a \`ToolError\` when it fails:
- \`await file(path).text()\`, \`.json()\`, \`.lines()\`, \`.exists()\`: the file's whole text, as it is on disk (no line numbers, nothing cut).
- \`await write(path, text)\` writes a file; \`await edit(path, oldText, newText, { all: true })\` replaces text that must match exactly once, or every match with \`all\`.
- \`await sleep(ms)\` waits; \`setTimeout(fn, ms, ...args)\` and \`clearTimeout(id)\` work as in a browser. A timer still waiting when its script ends is dropped, and a script that waits on a timer still stops when the user stops it or at its limits.
- \`\` await $\`git log -5\` \`\` runs a shell command through Bash (values written as \`\${...}\` are quoted for the shell) and throws when it exits with an error; \`\` (await $\`cmd\`.nothrow()).exitCode \`\` checks it instead.

What comes back to you, inside \`<repl-output>\`, is the value of the script's last expression in \`<result>\` (strings as they are, anything else as JSON, with the promises in a returned object's keys awaited), or the error that stopped it in \`<error>\`, after the lines it passed to \`self.log(...)\` in \`<log>\`; \`&\`, \`<\` and \`>\` in them are escaped. Everything else stays in the script: read files, search, filter and count there, and return only what you need to read. Each part is cut to about 30,000 characters, keeping its start and its end; \`preview(value, n)\` returns the first n characters of a long value with its size. \`console.log\` goes to a ring of the last lines, read back with \`console.tail(n)\`, never to you. Calls refused while a script ran are listed after its output. When the user answers No at a permission prompt a call raised, the script is stopped there and the output says so: ask the user before trying again. Output a script carried from an MCP server comes back marked as untrusted. \`await describeTool(name)\` returns a tool's whole declaration, \`await searchTools(words)\` finds tools by what they do.

Before writing a script that chains several steps, look in the script bank: \`await searchScripts("words about the flow")\` returns ready-made scripts with their input schemas and the tools they call (\`missing\` lists the ones you lack), and \`await scripts["name"](input)\` runs one inside this context, with your tools and the same permission checks, resolving to what it returns. Run a script that fits instead of redoing its flow call by call. When one almost fits, \`await adaptScript(name, goal, input)\` has a small model rewrite it for your goal (\`name\` may also be a script's source) and resolves to \`{ source, notes }\` without running anything: read the source, then run it with \`await runScript(source, input)\`.

Calls to read-only tools (Read, Grep, Glob, ...) started together with \`Promise.all\` run at the same time; edits, commands and anything that asks permission run one after another. Every call goes through the same permission rules, hooks and prompts as when you call the tool yourself.

The context persists: variables and functions a script declares stay for the next REPL call in this session, so keep results in variables instead of reading them again. A later call may declare a top-level \`const\`, \`let\` or \`class\` again, and a name a failed call declared stays usable (\`undefined\` when the failure came before its value), so keep your names instead of inventing new ones. The context is sealed: there is no \`import\`, \`require\`, \`process\` or \`fetch\`; reach files, commands and the web through the tools. Scripts run in strict mode.

A script that is still running after about 30 seconds is dispatched: the call returns \`(dispatched #N)\` with a task id, and the script goes on beside the scripts you send next, which share its variables. Its output comes back through TaskOutput, or by itself before your turn ends, so put the work that waits (a build, a test suite) in one script and go on with other things; send a script that depends on a dispatched one's result only after that result is back.

Limits: the script's own JavaScript may run for \`timeout\` without a break (30 seconds unless you ask for more; time waiting on tools does not count), the whole script stops after 10 minutes, tool calls included, and one script makes at most 1,000 tool calls.

Use REPL for work that would take many tool calls or that reads more than you need to see: searching across many files, applying the same edit in many places, collecting facts and returning a summary. A single call to one tool is simpler made directly.`,
};

type Input = { code?: unknown; description?: unknown; timeout?: unknown };

export function offered(ctx) {
  return typeof ctx?.codeMode === "string" && ctx.codeMode !== "off";
}

export function effects(_input: Input | null) {
  return [];
}

export function summary(input: Input | null) {
  const described = typeof input?.description === "string" ? input.description.trim() : "";
  if (described !== "") {
    return described;
  }
  const code = typeof input?.code === "string" ? input.code : "";
  return (code.split("\n")[0] ?? "").trim();
}

export function execute(input: Input, ctx) {
  const code = typeof input?.code === "string" ? input.code : "";
  if (code.trim() === "") {
    return { content: "Missing required parameter `code`.", isError: true };
  }
  const timeout = typeof input.timeout === "number" && Number.isInteger(input.timeout) && input.timeout >= 0
    ? input.timeout
    : null;
  return ctx.repl.eval({ code, timeout, summary: summary(input) });
}
