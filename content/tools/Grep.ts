const OUTPUT = 2000000;
const RESULTS = 200;
const PER_FILE = 25;
const LINE = 300;

export const spec = {
  name: "Grep",
  input: {
    "additionalProperties": false,
    "properties": {
      "-A": {
        "description": "Lines after each match (content mode).",
        "type": "integer"
      },
      "-B": {
        "description": "Lines before each match (content mode).",
        "type": "integer"
      },
      "-C": {
        "description": "Lines around each match (content mode).",
        "type": "integer"
      },
      "-i": {
        "description": "Ignore case.",
        "type": "boolean"
      },
      "-n": {
        "description": "Number lines (content mode).",
        "type": "boolean"
      },
      "glob": {
        "description": "Only files whose name matches this glob.",
        "type": "string"
      },
      "head_limit": {
        "description": "Show the first N results (default 200).",
        "type": "integer"
      },
      "multiline": {
        "description": "Let a match span lines; `.` then matches line ends too.",
        "type": "boolean"
      },
      "output_mode": {
        "enum": [
          "content",
          "files_with_matches",
          "count"
        ],
        "type": "string"
      },
      "path": {
        "description": "File or directory to search; the working directory if omitted.",
        "type": "string"
      },
      "pattern": {
        "description": "The regular expression.",
        "type": "string"
      }
    },
    "required": [
      "pattern"
    ],
    "type": "object"
  },
  description: `Search file contents with a regular expression (Rust regex syntax, like ripgrep). Files ignored by git, \`.ignore\` or \`.rgignore\` are skipped; hidden files are searched, the \`.git\` folder is not. \`output_mode\` is \`files_with_matches\` (default: the matching files), \`content\` (the matching lines; \`-n\` numbers them, \`-A\`/\`-B\`/\`-C\` add context) or \`count\`. Narrow with \`path\` and \`glob\` (e.g. \`*.rs\`). \`-i\` ignores case. Each line is matched on its own unless \`multiline\` is set: then a match can span lines, \`.\` matches line ends too, and \`content\` shows every line a match covers. Matching lines come grouped under their file's path; a file shows its first ${PER_FILE} matches, a line its first ${LINE} characters, and the whole answer the first ${RESULTS} results (\`head_limit\` changes that), each saying how much was left out. Always use this instead of running grep or rg through Bash.`,
  profile: { effects: ["read"], risk: "low", reversible: true, untrusted: true, sensitive: true },
};

type Input = {
  pattern: string;
  path?: string;
  glob?: string;
  output_mode?: string;
  multiline?: boolean;
  "-i"?: boolean;
  "-n"?: boolean;
  "-A"?: number;
  "-B"?: number;
  "-C"?: number;
  head_limit?: number;
};

type Found = { path: string; count?: number; hits?: number[]; total?: number; lines?: Record<string, string> };

export function effects(input: Input | null) {
  return [{ readTree: typeof input?.path === "string" ? input.path : "." }];
}

export function summary(input: Input | null) {
  return typeof input?.pattern === "string" ? input.pattern : "";
}

function count(value: unknown) {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 ? value : undefined;
}

function clip(text: string, limit: number) {
  if (text.length <= limit) {
    return text;
  }
  const chars = Array.from(text);
  if (chars.length <= limit) {
    return text;
  }
  return `${chars.slice(0, limit).join("")}\n… [${chars.length - limit} more characters cut]`;
}

function cut(line: string) {
  if (line.length <= LINE) {
    return line;
  }
  const chars = Array.from(line);
  if (chars.length <= LINE) {
    return line;
  }
  return `${chars.slice(0, LINE).join("")}… [+${chars.length - LINE} characters]`;
}

function matches(found: Found, numbered: boolean, before: number, after: number) {
  const hits = found.hits ?? [];
  const total = found.total ?? 0;
  const lines = found.lines ?? {};
  const isHit = new Set(hits);
  const out: string[] = [];
  let shownUntil = 0;
  for (const hit of hits.slice(0, PER_FILE)) {
    const start = Math.max(Math.max(hit - before, 0), shownUntil);
    const end = Math.max(Math.min(hit + after + 1, total), start);
    if (start > shownUntil && shownUntil > 0 && (before > 0 || after > 0)) {
      out.push("--");
    }
    for (let at = start; at < end; at++) {
      const line = cut(lines[String(at)] ?? "");
      out.push(numbered ? `${at + 1}${isHit.has(at) ? ":" : "-"}${line}` : line);
    }
    shownUntil = Math.max(shownUntil, end);
  }
  return { lines: out, more: Math.max(hits.length - PER_FILE, 0) };
}

function results(limit: number) {
  const shown: string[] = [];
  let kept = 0;
  let hidden = 0;
  return {
    shown,
    entry(entry: string) {
      if (kept < limit) {
        shown.push(entry);
        kept++;
      } else {
        hidden++;
      }
    },
    file(name: string, found: { lines: string[]; more: number }) {
      const room = Math.min(Math.max(limit - kept, 0), found.lines.length);
      hidden += found.lines.length - room;
      if (room === 0) {
        return;
      }
      kept += room;
      let group = `${name}\n${found.lines.slice(0, room).join("\n")}`;
      if (found.more > 0 && room === found.lines.length) {
        group += `\n… [${found.more} more matches in this file]`;
      }
      shown.push(group);
    },
    text(between: string) {
      let text = shown.join(between);
      if (hidden > 0) {
        text += `\n… [${hidden} more results; narrow the search or raise head_limit]`;
      }
      return text;
    },
  };
}

export function execute(input: Input, ctx) {
  if (typeof input?.pattern !== "string") {
    return { content: "Missing required parameter `pattern`.", isError: true };
  }
  const mode = input.output_mode === "content" ? "content" : input.output_mode === "count" ? "count" : "files";
  const context = count(input["-C"]) ?? 0;
  const before = count(input["-B"]) ?? context;
  const after = count(input["-A"]) ?? context;
  const limit = count(input.head_limit) || RESULTS;
  let found: Found[];
  try {
    found = ctx.search.grep({
      pattern: input.pattern,
      path: typeof input.path === "string" && input.path !== "" ? input.path : undefined,
      glob: typeof input.glob === "string" && input.glob.trim() !== "" ? input.glob : undefined,
      ignoreCase: input["-i"] === true,
      multiline: input.multiline === true,
      mode,
      before,
      after,
      most: PER_FILE,
    });
  } catch (error) {
    return { content: String(error.message ?? error), isError: true };
  }
  const listed = results(limit);
  for (const file of found) {
    if (mode === "content") {
      listed.file(file.path, matches(file, input["-n"] === true, before, after));
    } else if (mode === "count") {
      listed.entry(`${file.path}:${file.count}`);
    } else {
      listed.entry(file.path);
    }
  }
  if (listed.shown.length === 0) {
    return "No matches found.";
  }
  return clip(listed.text(mode === "content" ? "\n\n" : "\n"), OUTPUT);
}
