const SHOWN = 30000;

export const spec = {
  name: "BashOutput",
  input: {
    "additionalProperties": false,
    "properties": {
      "bash_id": {
        "description": "The id Bash returned.",
        "type": "string"
      },
      "filter": {
        "description": "Keep only lines containing this.",
        "type": "string"
      }
    },
    "required": [
      "bash_id"
    ],
    "type": "object"
  },
  description: `Read what a background command (Bash with run_in_background) printed since you last read it, and whether it is still running. Pass \`filter\` (a substring) to keep only the lines that contain it.`,
  profile: { effects: ["read"], risk: "low", reversible: true, untrusted: true, sensitive: false },
};

type Input = { bash_id: string; filter?: string };

type Read = { text: string; dropped: string | null; exit: string | null; seconds: number };

export function effects(_input: Input | null) {
  return [];
}

export function summary(input: Input | null) {
  return typeof input?.bash_id === "string" ? input.bash_id : "";
}

function lines(text: string) {
  const all = text.split("\n");
  if (all[all.length - 1] === "") {
    all.pop();
  }
  return all.map((line) => (line.endsWith("\r") ? line.slice(0, -1) : line));
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

function clip(text: string, limit: number) {
  const at = offset(text, limit);
  if (at >= text.length) {
    return text;
  }
  return `${text.slice(0, at)}\n… [${points(text.slice(at))} more characters cut]`;
}

export function execute(input: Input, ctx) {
  if (typeof input?.bash_id !== "string") {
    return { content: "Missing required parameter `bash_id`.", isError: true };
  }
  const id = input.bash_id;
  const read: Read | null = ctx.proc.read({ id });
  if (read === null) {
    return { content: `No background command \`${id}\`.`, isError: true };
  }
  const text = typeof input.filter === "string"
    ? lines(read.text).filter((line) => line.includes(input.filter as string)).join("\n")
    : read.text;
  const state = read.exit === null ? `running for ${read.seconds}s` : `finished (${read.exit})`;
  const shown = text.trim() === "" ? "(no new output)" : clip(text.trimEnd(), SHOWN);
  const dropped = read.dropped === null ? "" : `${read.dropped}\n`;
  const content = `[${id}: ${state}]\n${dropped}${shown}`;
  return read.exit === null ? content : { content, background: { Ended: id } };
}
