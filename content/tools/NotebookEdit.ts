export const spec = {
  name: "NotebookEdit",
  deferred: true,
  input: {
    "additionalProperties": false,
    "properties": {
      "cell_number": {
        "description": "The cell, from 0.",
        "type": "integer"
      },
      "cell_type": {
        "enum": [
          "code",
          "markdown"
        ]
      },
      "edit_mode": {
        "enum": [
          "replace",
          "insert",
          "delete"
        ]
      },
      "new_source": {
        "type": "string"
      },
      "notebook_path": {
        "type": "string"
      }
    },
    "required": [
      "notebook_path"
    ],
    "type": "object"
  },
  description: `Change one cell of a Jupyter notebook (.ipynb): \`replace\` its source (the default), \`insert\` a new cell after it (or first, without \`cell_number\`), or \`delete\` it. Cells are numbered from 0 as Read shows them. Read the notebook first.`,
  profile: { effects: ["write"], risk: "medium", reversible: "compensate", untrusted: false, sensitive: false },
};

type Input = {
  notebook_path: string;
  cell_number?: number;
  cell_type?: string;
  edit_mode?: string;
  new_source?: string;
};

export function effects(input: Input | null) {
  return typeof input?.notebook_path === "string" ? [{ write: input.notebook_path }] : [{ edits: true }];
}

export function summary(input: Input | null) {
  return typeof input?.notebook_path === "string" ? input.notebook_path : "";
}

function failed(content: string) {
  return { content, isError: true };
}

function sourceLines(text: string) {
  const lines: string[] = [];
  let start = 0;
  while (start < text.length) {
    const end = text.indexOf("\n", start);
    const next = end < 0 ? text.length : end + 1;
    lines.push(text.slice(start, next));
    start = next;
  }
  return lines;
}

function shapedLike(original: string, text: string) {
  const both = original.split("\r\n").length - 1;
  const bare = original.split("\n").length - 1 - both;
  const ending = both > bare ? "\r\n" : "\n";
  const body = text.endsWith("\r\n") ? text.slice(0, -2) : text.endsWith("\n") ? text.slice(0, -1) : text;
  const lf = body.split("\r\n").join("\n");
  const shaped = ending === "\n" ? lf : lf.split("\n").join("\r\n");
  return original.endsWith("\n") ? shaped + ending : shaped;
}

function cellNumber(value: unknown) {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 ? value : undefined;
}

function changed(cells: unknown[], input: Input) {
  const mode = typeof input.edit_mode === "string" ? input.edit_mode : "replace";
  const number = cellNumber(input.cell_number);
  const lines = sourceLines(typeof input.new_source === "string" ? input.new_source : "");
  const kind = typeof input.cell_type === "string" ? input.cell_type : undefined;
  if (mode === "insert") {
    const cell: Record<string, unknown> = { cell_type: kind ?? "code", metadata: {}, source: lines };
    if ((kind ?? "code") === "code") {
      cell.outputs = [];
      cell.execution_count = null;
    }
    cells.splice(Math.min(number === undefined ? 0 : number + 1, cells.length), 0, cell);
    return null;
  }
  if (mode === "delete") {
    if (number === undefined) {
      return "delete needs cell_number";
    }
    if (number >= cells.length) {
      return `the notebook has ${cells.length} cells`;
    }
    cells.splice(number, 1);
    return null;
  }
  if (mode !== "replace") {
    return `unknown edit_mode ${mode}`;
  }
  if (number === undefined) {
    return "replace needs cell_number";
  }
  const cell = cells[number] as Record<string, unknown> | undefined;
  if (cell === undefined) {
    return `the notebook has no cell ${number}`;
  }
  cell.source = lines;
  if (kind !== undefined) {
    cell.cell_type = kind;
  }
  if (cell.cell_type === "code") {
    cell.outputs = [];
    cell.execution_count = null;
  }
  return null;
}

function edited(text: string, input: Input) {
  let notebook;
  try {
    notebook = JSON.parse(text);
  } catch (error) {
    return { error: `not a notebook: ${error.message ?? error}` };
  }
  const cells = notebook?.cells;
  if (!Array.isArray(cells)) {
    return { error: "not a notebook: no cells" };
  }
  const refused = changed(cells, input);
  if (refused !== null) {
    return { error: refused };
  }
  return { text: shapedLike(text, `${JSON.stringify(notebook, null, 2)}\n`) };
}

export function execute(input: Input, ctx) {
  if (typeof input?.notebook_path !== "string") {
    return failed("Missing required parameter `notebook_path`.");
  }
  const path = input.notebook_path;
  const full = ctx.files.place({ path }).full;
  const freshness = ctx.files.freshness({ path });
  if (freshness === "unread") {
    return failed("Read the notebook before editing it.");
  }
  if (freshness === "changed") {
    return failed(
      `${full} changed on disk since you last read it (the user, a formatter or another tool wrote to it). Read it again before changing it, so your change does not overwrite theirs.`,
    );
  }
  let text: string;
  try {
    const read = ctx.fs.read({ path });
    if (typeof read.text !== "string") {
      return failed(`Could not read ${full}: stream did not contain valid UTF-8`);
    }
    text = read.text;
  } catch (error) {
    return failed(`Could not read ${full}: ${error.message ?? error}`);
  }
  const result = edited(text, input);
  if (result.error !== undefined) {
    return failed(result.error);
  }
  const notKept = ctx.files.checkpoint({ path });
  try {
    ctx.fs.write({ path, text: result.text });
  } catch (error) {
    const message = String(error.message ?? error);
    return failed(message.startsWith("Could not create ") ? message : `Could not write ${full}: ${message}`);
  }
  ctx.files.markRead({ path, whole: true });
  return `Updated ${full}.${notKept}`;
}
