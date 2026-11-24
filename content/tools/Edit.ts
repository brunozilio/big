const SHOWN_LINES = 500;

export const spec = {
  name: "Edit",
  input: {
    "additionalProperties": false,
    "properties": {
      "file_path": {
        "description": "The file to change.",
        "type": "string"
      },
      "new_string": {
        "description": "The text to put in its place.",
        "type": "string"
      },
      "old_string": {
        "description": "The exact text to replace.",
        "type": "string"
      },
      "replace_all": {
        "default": false,
        "description": "Replace every occurrence.",
        "type": "boolean"
      }
    },
    "required": [
      "file_path",
      "old_string",
      "new_string"
    ],
    "type": "object"
  },
  description: `Replace \`old_string\` with \`new_string\` in a file read earlier with Read. \`old_string\` must match the file exactly, indentation included (without Read's line-number prefix), and appear only once unless \`replace_all\` is true; include surrounding lines to make it unique. To create a file, use Write.`,
};

type Input = { file_path: string; old_string: string; new_string: string; replace_all?: boolean };

export function effects(input: Input | null) {
  return typeof input?.file_path === "string" ? [{ write: input.file_path }] : [{ edits: true }];
}

export function summary(input: Input | null) {
  return typeof input?.file_path === "string" ? input.file_path : "";
}

function failed(content: string) {
  return { content, isError: true };
}

function lineCount(text: string) {
  if (text === "") {
    return 0;
  }
  const parts = text.split("\n");
  return parts[parts.length - 1] === "" ? parts.length - 1 : parts.length;
}

function crlf(text: string) {
  const both = text.split("\r\n").length - 1;
  const bare = text.split("\n").length - 1 - both;
  return both > bare;
}

function asCrlf(text: string) {
  return text.split("\r\n").join("\n").split("\n").join("\r\n");
}

function replacementIn(file: string, old: string, replacement: string) {
  if (crlf(file)) {
    const shaped = { old: asCrlf(old), replacement: asCrlf(replacement) };
    if (file.includes(shaped.old)) {
      return shaped;
    }
  }
  return { old, replacement };
}

function numbered(number: number, mark: string, text: string) {
  return `${String(number).padStart(5)} ${mark} ${text}`;
}

function diff(heading: string, before: string, oldText: string, newText: string) {
  const first = before.split("\n").length;
  const old = oldText.split("\n");
  const changed = newText.split("\n");
  let sameStart = 0;
  while (sameStart < old.length && sameStart < changed.length && old[sameStart] === changed[sameStart]) {
    sameStart++;
  }
  let sameEnd = 0;
  while (
    sameEnd < old.length - sameStart &&
    sameEnd < changed.length - sameStart &&
    old[old.length - 1 - sameEnd] === changed[changed.length - 1 - sameEnd]
  ) {
    sameEnd++;
  }
  const removed = old.slice(sameStart, old.length - sameEnd);
  const added = changed.slice(sameStart, changed.length - sameEnd);
  const start = first + sameStart;
  const lines: string[] = [];
  if (sameStart > 0) {
    lines.push(numbered(start - 1, " ", old[sameStart - 1]));
  }
  removed.forEach((text, at) => lines.push(numbered(start + at, "-", text)));
  added.forEach((text, at) => lines.push(numbered(start + at, "+", text)));
  if (sameEnd > 0) {
    lines.push(numbered(start + added.length, " ", old[old.length - sameEnd]));
  }
  const more = lines.length - SHOWN_LINES;
  const kept = lines.slice(0, SHOWN_LINES);
  if (more > 0) {
    kept.push(`      … +${more} lines`);
  }
  return `${heading} · Added ${added.length} lines, removed ${removed.length}\n${kept.join("\n")}`;
}

function changedSinceRead(name: string) {
  return `${name} changed on disk since you last read it (the user, a formatter or another tool wrote to it). Read it again before changing it, so your change does not overwrite theirs.`;
}

function asked(input: Input) {
  for (const field of ["file_path", "old_string", "new_string"]) {
    if (typeof input?.[field] !== "string") {
      return `Missing required parameter \`${field}\`.`;
    }
  }
  if (input.old_string === "") {
    return "`old_string` is empty; give the exact text to replace, or use Write to create or overwrite the file.";
  }
  if (input.old_string === input.new_string) {
    return "`old_string` and `new_string` are the same; nothing to change.";
  }
  return null;
}

function current(path: string, name: string, ctx) {
  const freshness = ctx.files.freshness({ path });
  if (freshness === "unread") {
    return { error: `Read ${name} before editing it.` };
  }
  if (freshness === "changed") {
    return { error: changedSinceRead(name) };
  }
  try {
    const read = ctx.fs.read({ path });
    if (typeof read.text !== "string") {
      return { error: `Could not read ${name}: stream did not contain valid UTF-8` };
    }
    return { text: read.text as string };
  } catch (error) {
    return { error: `Could not read ${name}: ${error.message ?? error}` };
  }
}

function replaced(text: string, old: string, replacement: string, all: boolean, name: string) {
  const count = text.split(old).length - 1;
  if (count === 0) {
    return { error: `\`old_string\` was not found in ${name}. Read the file again and copy the text exactly.` };
  }
  if (count > 1 && !all) {
    return {
      error: `\`old_string\` appears ${count} times in ${name}. Include more surrounding text to make it unique, or set \`replace_all\`.`,
    };
  }
  return { changed: text.split(old).join(replacement), count };
}

export function execute(input: Input, ctx) {
  const refused = asked(input);
  if (refused !== null) {
    return failed(refused);
  }
  const path = input.file_path;
  const place = ctx.files.place({ path });
  const name = place.shown;
  const read = current(path, name, ctx);
  if (read.error !== undefined) {
    return failed(read.error);
  }
  const text = read.text;
  const { old, replacement } = replacementIn(text, input.old_string, input.new_string);
  if (old === replacement) {
    return failed("`old_string` and `new_string` differ only in line endings; nothing to change.");
  }
  const result = replaced(text, old, replacement, input.replace_all === true, name);
  if (result.error !== undefined) {
    return failed(result.error);
  }
  const before = text.slice(0, text.indexOf(old));
  const shown = diff(`Edited ${name}`, before, old, replacement);
  const notKept = ctx.files.checkpoint({ path });
  try {
    ctx.fs.write({ path, text: result.changed });
  } catch (error) {
    const message = String(error.message ?? error);
    return failed(message.startsWith("Could not create ") ? message : `Could not write ${name}: ${message}`);
  }
  const notes = ctx.files.wrote({ path, before: text, after: result.changed });
  const said = result.count === 1 ? `Edited ${name}.` : `Edited ${name} (${result.count} replacements).`;
  const edited = {
    path: place.full,
    line: before.split("\n").length,
    removed: lineCount(old),
    added: lineCount(replacement),
  };
  return { content: said + notes + notKept, shown, edited };
}
