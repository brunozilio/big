const DEFAULT_LINES = 2000;
const LINE_CHARS = 2000;
const MAX_CHARS = 40000;
const PDF_SHOWN = 40000;
const NOTEBOOK_OUTPUT = 4000;

export const spec = {
  name: "Read",
  input: {
    "additionalProperties": false,
    "properties": {
      "file_path": {
        "description": "The file to read.",
        "type": "string"
      },
      "limit": {
        "description": "How many lines to read.",
        "type": "integer"
      },
      "offset": {
        "description": "First line to read, from 1.",
        "type": "integer"
      },
      "pages": {
        "description": "A PDF's pages, like \"3\" or \"1-5\".",
        "type": "string"
      }
    },
    "required": [
      "file_path"
    ],
    "type": "object"
  },
  description: `Read a text file, a PNG, JPEG, GIF or WebP image, a PDF (as text; \`pages\` like "1-5" for a long one) or a Jupyter notebook (its cells and outputs) from the machine. Images are sent visually to the model. Give an absolute path, or one relative to the working directory. Returns up to ${DEFAULT_LINES} lines from the start, each prefixed with its line number and a tab; pass \`offset\` (first line, from 1) and \`limit\` to read a part of a long file. Lines longer than ${LINE_CHARS} characters are cut, and a read stops at the last whole line before ${MAX_CHARS} characters, saying which \`offset\` reads on; paging through every line of a file counts as reading it whole. Reading lines of an unchanged file that an earlier read already returned gives a short note instead of the text. When you know which part of a file you need, read only that part. Read a file before editing or overwriting it, but do not read it again to check an edit: Edit and Write fail when a change does not apply. To find files, use Glob; to search inside them, Grep. Several reads can be requested at once.`,
  profile: { effects: ["read"], risk: "low", reversible: true, untrusted: false, sensitive: false },
};

type Input = { file_path: string; offset?: number; limit?: number; pages?: string };

type Answer = string | { content: string; isError?: boolean; images?: unknown[] };

export function effects(input: Input | null) {
  return typeof input?.file_path === "string" ? [{ read: input.file_path }] : [];
}

export function summary(input: Input | null) {
  return typeof input?.file_path === "string" ? input.file_path : "";
}

function failed(content: string) {
  return { content, isError: true };
}

function count(value: unknown) {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 ? value : undefined;
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

function bytes(text: string) {
  let size = 0;
  for (let at = 0; at < text.length; at++) {
    const code = text.charCodeAt(at);
    if (code < 0x80) {
      size += 1;
    } else if (code < 0x800) {
      size += 2;
    } else if (code >= 0xd800 && code <= 0xdbff) {
      size += 4;
      at++;
    } else {
      size += 3;
    }
  }
  return size;
}

function clip(text: string, limit: number, hint = "") {
  if (text.length <= limit) {
    return text;
  }
  const chars = Array.from(text);
  if (chars.length <= limit) {
    return text;
  }
  return `${chars.slice(0, limit).join("")}\n… [${chars.length - limit} more characters cut${hint}]`;
}

function cut(line: string) {
  if (line.length <= LINE_CHARS) {
    return line;
  }
  const chars = Array.from(line);
  return chars.length > LINE_CHARS ? `${chars.slice(0, LINE_CHARS).join("")}… [line cut]` : line;
}

function joined(source: unknown) {
  if (Array.isArray(source)) {
    return source.filter((line) => typeof line === "string").join("");
  }
  return typeof source === "string" ? source : "";
}

function printed(outputs: unknown) {
  let text = "";
  for (const output of Array.isArray(outputs) ? outputs : []) {
    const said = output?.text;
    text += said === undefined || said === null ? joined(output?.data?.["text/plain"]) : joined(said);
    if (typeof output?.data?.["image/png"] === "string") {
      text += "[an image]\n";
    }
  }
  return text;
}

function notebook(text: string): Answer {
  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    return failed(`not a notebook: ${error.message ?? error}`);
  }
  const cells = parsed?.cells;
  if (!Array.isArray(cells)) {
    return failed("not a notebook: no cells");
  }
  let out = "";
  cells.forEach((cell, at) => {
    const kind = typeof cell?.cell_type === "string" ? cell.cell_type : "code";
    const id = typeof cell?.id === "string" ? ` id=${cell.id}` : "";
    out += `<cell ${at}${id} ${kind}>\n${joined(cell?.source).trimEnd()}\n`;
    const shown = printed(cell?.outputs);
    if (shown.trim() !== "") {
      out += `<output>\n${clip(shown.trimEnd(), NOTEBOOK_OUTPUT)}\n</output>\n`;
    }
    out += "</cell>\n";
  });
  return out;
}

function range(pages: string) {
  const invalid = `pages must be a page or a range like "1-5", not ${JSON.stringify(pages)}`;
  const dash = pages.indexOf("-");
  const [first, last] = dash < 0 ? [pages, pages] : [pages.slice(0, dash), pages.slice(dash + 1)];
  const page = (text: string) => (/^\+?\d+$/.test(text.trim()) ? Number(text.trim()) : NaN);
  const [from, to] = [page(first), page(last)];
  if (Number.isNaN(from) || Number.isNaN(to) || from > 0xffffffff || to > 0xffffffff) {
    return { error: invalid };
  }
  if (from === 0 || to < from || to - from >= 50) {
    return { error: `${invalid} (at most 50 pages at a time)` };
  }
  return { first: from, last: to };
}

function pdf(input: Input, ctx): Answer {
  let asked = {};
  if (typeof input.pages === "string") {
    const pages = range(input.pages);
    if ("error" in pages) {
      return failed(pages.error);
    }
    asked = pages;
  }
  let text: string;
  try {
    text = ctx.media.pdf({ path: input.file_path, ...asked });
  } catch (error) {
    return failed(String(error.message ?? error));
  }
  let out = "";
  text.split("\f").forEach((page, at) => {
    if (page.trim() !== "") {
      out += `--- page ${at + 1} ---\n${page.trimEnd()}\n`;
    }
  });
  return clip(out, PDF_SHOWN, "; read a smaller `pages` range to see the rest");
}

function raw(input: Input, shown: string, ctx): Answer {
  if (ctx.fs.stat({ path: input.file_path }).dir) {
    return failed(`${shown} is a directory, not a file.`);
  }
  let loaded;
  try {
    loaded = ctx.media.load({ path: input.file_path, plain: true });
  } catch (error) {
    return failed(`Could not read ${shown}: ${error.message ?? error}`);
  }
  if (typeof loaded.text !== "string") {
    return failed(`${shown} is not a text file.`);
  }
  ctx.files.markRead({ path: input.file_path, whole: true });
  return loaded.text;
}

function numbered(text: string, path: string, shown: string, input: Input, ctx): Answer {
  const offset = Math.max(count(input.offset) ?? 1, 1);
  const limit = count(input.limit) ?? DEFAULT_LINES;
  const all = lines(text);
  const total = all.length;
  if (offset > total) {
    return failed(`The file has ${total} lines; offset ${offset} is past its end.`);
  }
  let last = Math.min(offset - 1 + limit, total);
  if (ctx.files.seen({ path, first: offset, last })) {
    return `${shown} is unchanged since you last read these lines in this conversation; that text is still current.`;
  }
  let out = "";
  let size = 0;
  for (let number = offset - 1; number < Math.min(offset - 1 + limit, total); number++) {
    const line = `${String(number + 1).padStart(6)}\t${cut(all[number])}\n`;
    const length = bytes(line);
    if (size + length > MAX_CHARS && out !== "") {
      last = number;
      break;
    }
    out += line;
    size += length;
  }
  ctx.files.saw({ path, first: offset, last });
  if (offset > 1 && ctx.files.seen({ path, first: 1, last: total })) {
    ctx.files.markRead({ path, whole: true });
  }
  return out + rest(total, last, last < Math.min(offset - 1 + limit, total));
}

function rest(total: number, last: number, capped: boolean) {
  if (last >= total) {
    return "";
  }
  if (capped) {
    return `… [stopped at line ${last} of ${total}: one read returns at most ${MAX_CHARS} characters. Read on from offset ${last + 1} (a \`limit\` keeps each part small), or Grep for the part you need]\n`;
  }
  return `… [${total - last} more lines; read on from offset ${last + 1}]\n`;
}

function notebookPath(full: string) {
  const name = full.slice(full.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  return dot > 0 && name.slice(dot + 1) === "ipynb";
}

function read(input: Input, place: { shown: string; full: string }, ctx): Answer {
  const path = input.file_path;
  const shown = place.shown;
  if (ctx.fs.stat({ path }).dir) {
    return failed(`${shown} is a directory; use Glob or Bash \`ls\` to list it.`);
  }
  let loaded;
  try {
    loaded = ctx.media.load({ path });
  } catch (error) {
    return failed(`Could not read ${shown}: ${error.message ?? error}`);
  }
  if (loaded.image) {
    ctx.files.markRead({ path, whole: true });
    return { content: `Image (${loaded.image.media_type})`, images: [loaded.image] };
  }
  if (typeof loaded.imageError === "string") {
    return failed(loaded.imageError);
  }
  if (loaded.pdf) {
    ctx.files.markRead({ path, whole: true });
    return pdf(input, ctx);
  }
  if (typeof loaded.text !== "string") {
    return failed(`${shown} is not a text file.`);
  }
  const text: string = loaded.text;
  const offset = Math.max(count(input.offset) ?? 1, 1);
  const limit = count(input.limit) ?? DEFAULT_LINES;
  const whole = offset === 1 && lines(text).length <= limit && bytes(text) <= MAX_CHARS;
  ctx.files.markRead({ path, whole });
  if (notebookPath(place.full)) {
    return notebook(text);
  }
  if (text === "") {
    return "(the file is empty)";
  }
  return numbered(text, path, shown, input, ctx);
}

export function execute(input: Input, ctx) {
  if (typeof input?.file_path !== "string") {
    return failed("Missing required parameter `file_path`.");
  }
  const place = ctx.files.place({ path: input.file_path });
  if (ctx.files.raw()) {
    return raw(input, place.shown, ctx);
  }
  const answer = read(input, place, ctx);
  if (typeof answer === "string") {
    return answer + ctx.files.anchored({ path: input.file_path });
  }
  if (answer.isError) {
    return answer;
  }
  return { ...answer, content: answer.content + ctx.files.anchored({ path: input.file_path }) };
}
