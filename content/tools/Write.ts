const SHOWN_LINES = 500;

export const spec = {
  name: "Write",
  input: {
    "additionalProperties": false,
    "properties": {
      "content": {
        "description": "The file's whole new content.",
        "type": "string"
      },
      "file_path": {
        "description": "The file to write.",
        "type": "string"
      }
    },
    "required": [
      "file_path",
      "content"
    ],
    "type": "object"
  },
  description: `Create a file, or replace one entirely, with \`content\`. Missing parent directories are created. An existing file must have been read with Read first. Prefer Edit to change part of a file; use Write for new files or complete rewrites.`,
};

type Input = { file_path: string; content: string };

export function effects(input: Input | null) {
  return typeof input?.file_path === "string" ? [{ write: input.file_path }] : [{ edits: true }];
}

export function summary(input: Input | null) {
  return typeof input?.file_path === "string" ? input.file_path : "";
}

function failed(content: string) {
  return { content, isError: true };
}

function lines(text: string) {
  if (text === "") {
    return [];
  }
  const parts = text.split("\n");
  const ended = parts[parts.length - 1] === "";
  if (ended) {
    parts.pop();
  }
  const last = ended ? parts.length : parts.length - 1;
  return parts.map((line, at) => (at < last && line.endsWith("\r") ? line.slice(0, -1) : line));
}

function created(heading: string, content: string) {
  const shown = lines(content).map((text, at) => `${String(at + 1).padStart(5)} + ${text}`);
  const counts = `Wrote ${shown.length} lines`;
  const more = shown.length - SHOWN_LINES;
  const kept = shown.slice(0, SHOWN_LINES);
  if (more > 0) {
    kept.push(`      … +${more} lines`);
  }
  return `${heading} · ${counts}\n${kept.join("\n")}`;
}

function changedSinceRead(name: string) {
  return `${name} changed on disk since you last read it (the user, a formatter or another tool wrote to it). Read it again before changing it, so your change does not overwrite theirs.`;
}

function refusal(name: string, path: string, ctx) {
  switch (ctx.files.freshness({ path })) {
    case "unread":
      return `${name} already exists; Read it before overwriting it.`;
    case "changed":
      return changedSinceRead(name);
    case "partial":
      return `You read only part of ${name}, and Write replaces all of it, so the rest would be lost. Read the whole file first, or change it with Edit.`;
    default:
      return null;
  }
}

function previous(path: string, ctx) {
  try {
    const read = ctx.fs.read({ path });
    return typeof read.text === "string" ? read.text : "";
  } catch {
    return "";
  }
}

export function execute(input: Input, ctx) {
  for (const field of ["file_path", "content"]) {
    if (typeof input?.[field] !== "string") {
      return failed(`Missing required parameter \`${field}\`.`);
    }
  }
  const path = input.file_path;
  const content = input.content;
  const place = ctx.files.place({ path });
  const name = place.shown;
  const existed = ctx.fs.stat({ path }).exists === true;
  if (existed) {
    const refused = refusal(name, path, ctx);
    if (refused !== null) {
      return failed(refused);
    }
  }
  const before = existed ? previous(path, ctx) : null;
  const notKept = ctx.files.checkpoint({ path });
  try {
    ctx.fs.write({ path, text: content });
  } catch (error) {
    const message = String(error.message ?? error);
    return failed(message.startsWith("Could not create ") ? message : `Could not write ${name}: ${message}`);
  }
  const notes = ctx.files.wrote({ path, before: before ?? "", after: content });
  const count = lines(content).length;
  const heading = existed ? `Replaced ${name}` : `Created ${name}`;
  return {
    content: `${heading} (${count} lines).${notes}${notKept}`,
    shown: created(heading, content),
    edited: { path: place.full, line: 1, removed: before === null ? 0 : lines(before).length, added: count },
  };
}
