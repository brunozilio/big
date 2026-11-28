const DEFAULT_TIMEOUT = 120000;
const MAX_TIMEOUT = 600000;
const OUTPUT = 30000;
const NAMED_AT_MOST = 5;

function zshNote(shell?: string) {
  if (typeof shell !== "string" || !/zsh/i.test(shell)) {
    return "";
  }
  return " In zsh, \`status\` is a read-only variable (it holds the last exit code), so \`status=...\` fails: name it something else, such as \`state\`. Unquoted globs that match nothing are an error, and \`$var\` is not split into words.";
}

export const spec = {
  name: "Bash",
  input: {
    "additionalProperties": false,
    "properties": {
      "command": {
        "description": "The command to run.",
        "type": "string"
      },
      "description": {
        "description": "What the command does, in plain active words the user reads, often without seeing the command: do not echo its text, flags or paths. Five to ten words for simple commands (`git status` → \"Show working tree status\"); more context for pipes or obscure flags (`git reset --hard origin/main` → \"Discard all local changes and match remote main\"). Never call a command \"complex\" or \"risky\": say what it does.",
        "type": "string"
      },
      "run_in_background": {
        "description": "Leave it running and return its id at once.",
        "type": "boolean"
      },
      "timeout": {
        "description": "Milliseconds before it is stopped.",
        "type": "integer"
      }
    },
    "required": [
      "command"
    ],
    "type": "object"
  },
  description: (ctx: { shell?: string }) => `Run a shell command in the working directory and return its output (stdout and stderr) and exit code. Each call starts a fresh shell, so \`cd\` and variables do not carry over; prefer absolute paths. The command is stopped after \`timeout\` milliseconds (default ${DEFAULT_TIMEOUT}, at most ${MAX_TIMEOUT}). Output past ${OUTPUT} characters (unless the user set another limit) is cut: you see its start and its end, under a first line saying how much was cut and the file that holds all of it, which you can Read or Grep. Use Read, Glob, Grep, Edit and Write for files instead of cat, find, grep, sed or echo. Quote paths with spaces. Its output reaches you, not reliably the user. This computer runs commands with ${ctx.shell ?? "the user's shell"}.${zshNote(ctx.shell)}

Background: set \`run_in_background\` for something that keeps running (a dev server, a watcher, a long build). It returns an id at once; read what it printed with BashOutput and stop it with KillShell. It keeps running across turns until stopped or until the user leaves. Never use \`&\` or \`nohup\` for this.

Git:
- Interactive flags (\`git rebase -i\`, \`git add -i\`) do not work here: there is no terminal.
- Use the \`gh\` CLI for GitHub (pull requests, issues, the API).
- GitHub notifies the owner of any \`@name\` written in a pull request, issue or comment. Write a bare \`@name\` only for a real login you mean to notify; put roles, placeholders and code (\`@reviewer\`, \`@types/node\`) in backticks or drop the \`@\`.
- Commit or push only when the user asks. On the default branch, create a branch first.
- Never force-push, rewrite published history or skip hooks unless the user asks for that.`,
  profile: { effects: ["write", "send-external", "destructive"], risk: "high", reversible: false, untrusted: false, sensitive: false },
};

type Input = { command: string; description?: string; run_in_background?: boolean; timeout?: number };

type Policy = { cleanup: boolean; filters: boolean; savings: boolean; limit: number };

type Waited = {
  ended: "exited" | "detached" | "timedOut" | "stopped";
  output?: string;
  success?: boolean;
  code?: number | null;
  notRestorable: string[];
};

export function effects(input: Input | null) {
  return [{ exec: typeof input?.command === "string" ? input.command : "" }];
}

export function summary(input: Input | null) {
  return typeof input?.command === "string" ? input.command : "";
}

function failed(content: string) {
  return { content, isError: true };
}

function offset(text: string, count: number) {
  let at = 0;
  for (let seen = 0; seen < count && at < text.length; seen++) {
    const code = text.charCodeAt(at);
    at += code >= 0xd800 && code <= 0xdbff && at + 1 < text.length ? 2 : 1;
  }
  return at;
}

function points(text: string) {
  let count = 0;
  for (let at = 0; at < text.length; count++) {
    const code = text.charCodeAt(at);
    at += code >= 0xd800 && code <= 0xdbff && at + 1 < text.length ? 2 : 1;
  }
  return count;
}

function lineCount(text: string) {
  if (text === "") {
    return 0;
  }
  const lines = text.split("\n");
  if (lines[lines.length - 1] === "") {
    lines.pop();
  }
  return lines.length;
}

function cut(text: string, limit: number, saved: string | null) {
  const total = points(text);
  const head = Math.floor(limit / 2);
  const tail = limit - head;
  const headEnd = offset(text, head);
  const tailStart = offset(text, total - tail);
  const whole = saved === null
    ? "The full output could not be saved."
    : `The full output is in ${saved}: Read or Grep it for the rest.`;
  return `[Output cut: ${total} characters in ${lineCount(text)} lines, over the limit of ${limit}; the first ${head} and the last ${tail} are shown. ${whole}]\n${text.slice(0, headEnd)}\n… [${total - limit} characters cut here] …\n${text.slice(tailStart)}`;
}

function limited(text: string, policy: Policy, ctx) {
  if (offset(text, policy.limit) >= text.length) {
    return text;
  }
  return cut(text, policy.limit, ctx.proc.keep({ text }));
}

function shaped(command: string, output: string, policy: Policy, ctx) {
  let text = policy.cleanup ? ctx.proc.clean({ text: output }) : output;
  if (policy.filters) {
    const filtered = ctx.proc.filter({ command, text });
    const whole = filtered === null ? null : ctx.proc.keep({ text });
    if (filtered !== null && whole !== null) {
      const footer = `[${filtered.hidden} lines of ${filtered.by.join(", ")} noise hidden (passing tests, progress, build steps, repeated warnings); the whole output is in ${whole}]`;
      text = `${filtered.text}\n${footer}`;
    }
  }
  if (policy.savings) {
    ctx.proc.saving({ command, before: points(output), after: points(text) });
  }
  return text;
}

function notKept(skipped: string[]) {
  if (skipped.length === 0) {
    return null;
  }
  const named = skipped.slice(0, NAMED_AT_MOST);
  const more = skipped.length - named.length;
  const rest = more > 0 ? ` and ${more} more` : "";
  return `(/rewind cannot restore ${named.join(", ")}${rest}: the repository runs a filter on them that was not run here, so what git stores is not what was checked out)`;
}

function timeoutOf(input: Input) {
  const asked = input.timeout;
  const valid = typeof asked === "number" && Number.isInteger(asked) && asked >= 0;
  return Math.min(valid ? asked : DEFAULT_TIMEOUT, MAX_TIMEOUT);
}

function background(command: string, policy: Policy, ctx) {
  let id: string;
  try {
    id = ctx.proc.spawn({ command, git: true, plain: policy.cleanup }).id;
  } catch (error) {
    return failed(`Could not start the shell: ${error.message ?? error}`);
  }
  ctx.proc.watch({ id, lines: false });
  return {
    content: `Started in the background as ${id}. Read its output with BashOutput and stop it with KillShell; you will be told when it ends.`,
    background: { Started: id },
  };
}

function ended(command: string, id: string, waited: Waited, timeout: number, policy: Policy, ctx) {
  const output = waited.output ?? "";
  switch (waited.ended) {
    case "exited": {
      const text = limited(shaped(command, output, policy, ctx), policy, ctx);
      if (waited.success) {
        return { content: text === "" ? "(no output)" : text };
      }
      const code = waited.code ?? "a signal";
      return failed(`${text}\n(exit code ${code})`.trimStart());
    }
    case "detached":
      ctx.proc.watch({ id, lines: false });
      return {
        content: `The user moved this command to the background, where it goes on as ${id}. Read its output with BashOutput and stop it with KillShell; you will be told when it ends.`,
      };
    case "stopped":
      return failed(
        `${limited(output, policy, ctx)}\n\nStopped while running: the command may have run in part or in full, and its changes may have been written. Check what it did before running it again.`,
      );
    default:
      return failed(`${limited(output, policy, ctx)}\n(stopped after ${timeout} ms)`.trimStart());
  }
}

export function execute(input: Input, ctx) {
  if (typeof input?.command !== "string") {
    return failed("Missing required parameter `command`.");
  }
  const command = input.command;
  const policy: Policy = ctx.proc.policy();
  if (input.run_in_background === true) {
    return background(command, policy, ctx);
  }
  const timeout = timeoutOf(input);
  const snapshot = ctx.git.snapshot();
  let id: string;
  try {
    id = ctx.proc.spawn({ command, git: true, plain: policy.cleanup }).id;
  } catch (error) {
    if (snapshot !== null) {
      ctx.git.forget({ id: snapshot.id });
    }
    return failed(`Could not start the shell: ${error.message ?? error}`);
  }
  const waited: Waited = ctx.proc.wait({ id, timeout, changes: snapshot?.id });
  const outcome: { content: string; isError?: boolean } = ended(command, id, waited, timeout, policy, ctx);
  const note = notKept(waited.notRestorable);
  if (note !== null) {
    outcome.content = `${outcome.content}\n${note}`;
  }
  return outcome;
}
