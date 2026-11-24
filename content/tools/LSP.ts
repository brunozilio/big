export const spec = {
  name: "LSP",
  input: {
    "additionalProperties": false,
    "properties": {
      "character": {
        "description": "Column of the symbol, from 1.",
        "type": "integer"
      },
      "file_path": {
        "description": "The file the question is about.",
        "type": "string"
      },
      "line": {
        "description": "Line of the symbol, from 1.",
        "type": "integer"
      },
      "operation": {
        "enum": [
          "goToDefinition",
          "findReferences",
          "hover",
          "documentSymbol",
          "workspaceSymbol",
          "goToImplementation",
          "incomingCalls",
          "outgoingCalls",
          "diagnostics",
          "workspaceDiagnostics"
        ],
        "type": "string"
      },
      "query": {
        "description": "The name to find (workspaceSymbol).",
        "type": "string"
      }
    },
    "required": [
      "operation",
      "file_path"
    ],
    "type": "object"
  },
  description: `Ask the file's language server about code: \`goToDefinition\`, \`findReferences\`, \`goToImplementation\`, \`hover\` (type and docs), \`incomingCalls\`/\`outgoingCalls\` (who calls a function, what it calls) at a position; \`documentSymbol\` (a file's outline) or \`workspaceSymbol\` (find a symbol by name across the project, with \`query\`). \`line\` and \`character\` count from 1, as Read shows them; point at the symbol's name. Prefer this over Grep to find where a symbol is defined or used: it follows the language's rules, not the text. To find a function or type by name, or to list a project's declarations, ask \`workspaceSymbol\` instead of a Grep for the language's keywords. Before reading a long file, ask \`documentSymbol\` and Read only the part you need; a short file is quicker read whole. \`diagnostics\` gives the file's current errors and warnings as the server sees them (the file is sent as it is on disk first); \`workspaceDiagnostics\` gives every error and warning the file's server has reported for the project so far, not only for that file. Both answer as one line per problem, errors first. Ask them after a change you are unsure of, instead of running a full build. The first question to a project can wait while its server starts.`,
};

const OUTPUT = 30000;
const MAX_ENTRIES = 100;
const MAX_LISTED = 50;
const DIAGNOSTICS_WAIT = 10;

const OPERATIONS = {
  goToDefinition: { method: "textDocument/definition", nouns: ["definition", "definitions"], single: true },
  findReferences: { method: "textDocument/references", nouns: ["reference", "references"] },
  hover: { method: "textDocument/hover", nouns: ["symbol", "symbols"] },
  documentSymbol: { method: "textDocument/documentSymbol", nouns: ["symbol", "symbols"], anywhere: true, symbols: true },
  workspaceSymbol: { method: "workspace/symbol", nouns: ["symbol", "symbols"], anywhere: true, symbols: true },
  goToImplementation: { method: "textDocument/implementation", nouns: ["implementation", "implementations"], single: true },
  incomingCalls: { method: "textDocument/prepareCallHierarchy", nouns: ["caller", "callers"], then: "callHierarchy/incomingCalls", side: "from" },
  outgoingCalls: { method: "textDocument/prepareCallHierarchy", nouns: ["callee", "callees"], then: "callHierarchy/outgoingCalls", side: "to" },
};

const SCOPES = ["diagnostics", "workspaceDiagnostics"];

const KINDS = [
  "file", "module", "namespace", "package", "class", "method", "property", "field", "constructor", "enum",
  "interface", "function", "variable", "constant", "string", "number", "boolean", "array", "object", "key", "null",
  "enum member", "struct", "event", "operator", "type parameter",
];

type Input = { operation?: string; file_path?: string; line?: number; character?: number; query?: string };
type Query = { operation: string; file: string; line: number; column: number; query: string };

export function effects(input: Input | null) {
  return typeof input?.file_path === "string" ? [{ read: input.file_path }, { installFor: input.file_path }] : [];
}

export function offered(ctx) {
  return ctx.services?.lsp === true;
}

export function summary(input: Input | null) {
  const text = (value: unknown) => (typeof value === "string" ? value : "");
  return `${text(input?.operation)} ${text(input?.file_path)}`;
}

function object(value: unknown) {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value : null;
}

function at(value: unknown, key: string) {
  const fields = object(value);
  return fields !== null && key in fields ? fields[key] : null;
}

function whole(value: unknown) {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 ? value : null;
}

function parse(input: Input): Query {
  const text = (name: string) => (typeof input[name] === "string" ? input[name] : null);
  const operation = text("operation") ?? "";
  if (!(operation in OPERATIONS)) {
    const known = [...Object.keys(OPERATIONS), ...SCOPES].join(", ");
    throw new Error(`Unknown operation \`${operation}\`; use one of: ${known}.`);
  }
  const file = text("file_path");
  if (file === null) {
    throw new Error("Missing required parameter `file_path`.");
  }
  const query = text("query") ?? "";
  if (operation === "workspaceSymbol" && query.trim() === "") {
    throw new Error("workspaceSymbol needs a `query`: the name to search for.");
  }
  const number = (name: string) => {
    const value = whole(input[name]);
    if (value === null) {
      throw new Error(`${name} needs \`line\` and \`character\`.`);
    }
    if (value < 1 || value > 4294967295) {
      throw new Error(`\`${name}\` counts from 1.`);
    }
    return value;
  };
  const anywhere = OPERATIONS[operation].anywhere === true;
  const line = anywhere ? 1 : number("line");
  const column = anywhere ? 1 : number("character");
  return { operation, file, line, column, query };
}

function asked(query: Query, uri: string, lineText: string) {
  const document = { uri };
  const character = [...lineText].slice(0, query.column - 1).reduce((sum, unit) => sum + unit.length, 0);
  const position = { textDocument: document, position: { line: query.line - 1, character } };
  const method = OPERATIONS[query.operation].method;
  switch (query.operation) {
    case "findReferences":
      return { method, params: { ...position, context: { includeDeclaration: false } } };
    case "documentSymbol":
      return { method, params: { textDocument: document } };
    case "workspaceSymbol":
      return { method, params: { query: query.query } };
    default:
      return { method, params: position };
  }
}

function explained(failed: { failed: string; busy: boolean }) {
  return failed.busy
    ? `${failed.failed}. The language server is still indexing the project. Do not repeat this identical call right away: carry on with Read or Grep, and ask the language server again after other work.`
    : failed.failed;
}

function requested(ctx, file: string, method: string, params: unknown) {
  const reply = ctx.lsp.request({ path: file, method, params });
  if ("failed" in reply) {
    throw new Error(explained(reply));
  }
  return reply.answer;
}

function named(item: unknown) {
  const name = typeof at(item, "name") === "string" ? at(item, "name") : "?";
  const kind = whole(at(item, "kind"));
  const label = kind !== null && kind >= 1 ? KINDS[kind - 1] : undefined;
  return label === undefined ? name : `${label} ${name}`;
}

function located(ctx, uri: unknown, range: unknown, withText: boolean) {
  const start = at(range, "start");
  const line = whole(at(start, "line"));
  const character = whole(at(start, "character"));
  if (typeof uri !== "string" || line === null || line > 4294967295 || character === null) {
    return null;
  }
  const found = ctx.lsp.where(withText ? { uri, line } : { uri });
  if (found === null) {
    return null;
  }
  return { at: `${found.shown}:${line + 1}:${character + 1}`, text: found.text };
}

function place(ctx, item: unknown) {
  const linked = object(item) !== null && "targetUri" in object(item);
  const uri = linked ? at(item, "targetUri") : at(item, "uri");
  const selection = object(item) !== null && "targetSelectionRange" in object(item);
  const range = linked ? (selection ? at(item, "targetSelectionRange") : at(item, "targetRange")) : at(item, "range");
  const found = located(ctx, uri, range, true);
  if (found === null) {
    return null;
  }
  return typeof found.text === "string" && found.text.trim() !== "" ? `${found.at}: ${found.text.trim()}` : found.at;
}

function hover(answer: unknown) {
  const text = (part: unknown) => (typeof part === "string" ? part : typeof at(part, "value") === "string" ? at(part, "value") : null);
  const contents = at(answer, "contents");
  const parts = (Array.isArray(contents) ? contents : [contents]).map(text).filter((part) => part !== null);
  const joined = parts.join("\n\n");
  return joined.trim() === "" ? null : joined;
}

function symbol(ctx, item: unknown) {
  const location = at(item, "location");
  const uri = at(location, "uri");
  if (typeof uri !== "string") {
    return null;
  }
  const found = located(ctx, uri, at(location, "range"), false);
  if (found !== null) {
    return `${named(item)} ${found.at}`;
  }
  const where = ctx.lsp.where({ uri });
  return where === null ? null : `${named(item)} ${where.shown}`;
}

function outline(ctx, item: unknown, depth: number): string[] {
  if (object(item) !== null && "location" in object(item)) {
    const found = symbol(ctx, item);
    return found === null ? [] : [found];
  }
  const selection = object(item) !== null && "selectionRange" in object(item);
  const range = selection ? at(item, "selectionRange") : at(item, "range");
  const line = whole(at(at(range, "start"), "line"));
  if (line === null) {
    return [];
  }
  const children = Array.isArray(at(item, "children")) ? at(item, "children") : [];
  const nested = children.flatMap((child) => outline(ctx, child, depth + 1));
  return [`${"  ".repeat(depth)}${named(item)} (line ${line + 1})`, ...nested];
}

function callItem(ctx, item: unknown) {
  const selection = object(item) !== null && "selectionRange" in object(item);
  const range = selection ? at(item, "selectionRange") : at(item, "range");
  const found = located(ctx, at(item, "uri"), range, false);
  return found === null ? null : `${named(item)} ${found.at}`;
}

function entries(ctx, operation: string, items: unknown[]) {
  const side = OPERATIONS[operation].side;
  const each = (item: unknown) => {
    if (side !== undefined) {
      return [callItem(ctx, at(item, side))];
    }
    if (operation === "documentSymbol") {
      return outline(ctx, item, 0);
    }
    return [operation === "workspaceSymbol" ? symbol(ctx, item) : place(ctx, item)];
  };
  return items.flatMap(each).filter((entry) => entry !== null);
}

function describe(ctx, operation: string, answer: unknown) {
  if (operation === "hover") {
    return hover(answer) ?? "No hover information.";
  }
  const items = answer === null ? [] : Array.isArray(answer) ? answer : [answer];
  const found = entries(ctx, operation, items);
  const listed = found.length > MAX_ENTRIES
    ? `${found.slice(0, MAX_ENTRIES).join("\n")}\n… and ${found.length - MAX_ENTRIES} more`
    : found.join("\n");
  const { nouns, single, symbols } = OPERATIONS[operation];
  const [one, many] = nouns;
  if (found.length === 0) {
    return single ? `No ${one} found.` : `No ${many} found.`;
  }
  if (symbols || (found.length === 1 && single)) {
    return listed;
  }
  return found.length === 1 ? `1 ${one}:\n${listed}` : `${found.length} ${many}:\n${listed}`;
}

function counted(count: number, one: string) {
  return count === 1 ? `1 ${one}` : `${count} ${one}s`;
}

function compared(a: unknown[], b: unknown[]) {
  for (let index = 0; index < a.length; index += 1) {
    if (a[index] < b[index]) return -1;
    if (a[index] > b[index]) return 1;
  }
  return 0;
}

function problems(files: { file: string; items: unknown[] }[]) {
  const found = [];
  for (const { file, items } of files) {
    for (const item of items) {
      const severity = whole(at(item, "severity")) ?? 1;
      const start = at(at(item, "range"), "start");
      const message = at(item, "message");
      const line = whole(at(start, "line"));
      const character = whole(at(start, "character"));
      if (typeof message === "string" && line !== null && character !== null && severity <= 2) {
        found.push([severity, file, line, character, message.split(/\s+/).filter((word) => word !== "").join(" ")]);
      }
    }
  }
  return found.sort(compared);
}

function listed(files: { file: string; items: unknown[] }[]) {
  const found = problems(files);
  if (found.length === 0) {
    return null;
  }
  const errors = found.filter((problem) => problem[0] === 1).length;
  const touched = new Set(found.map((problem) => problem[1])).size;
  const lines = [`${counted(errors, "error")} and ${counted(found.length - errors, "warning")} in ${counted(touched, "file")}`];
  for (const [severity, file, line, character, message] of found.slice(0, MAX_LISTED)) {
    lines.push(`${file}:${line + 1}:${character + 1} ${severity === 1 ? "error" : "warning"}: ${message}`);
  }
  if (found.length > MAX_LISTED) {
    lines.push(`… and ${found.length - MAX_LISTED} more`);
  }
  return lines.join("\n");
}

function withInstall(installed: string | null, answer: string) {
  return installed === null ? answer : `Installed the ${installed} language server on first use.\n\n${answer}`;
}

function diagnose(ctx, operation: string, input: Input) {
  if (typeof input.file_path !== "string") {
    throw new Error("Missing required parameter `file_path`.");
  }
  const opened = ctx.lsp.open({ path: input.file_path });
  if (operation === "workspaceDiagnostics") {
    const files = ctx.lsp.published({ path: input.file_path });
    const answer = listed(files) ?? "The language server reports no errors or warnings in the files it has checked.";
    return withInstall(opened.installed, answer);
  }
  const items = ctx.lsp.diagnostics({ path: input.file_path, version: opened.version, waitSeconds: DIAGNOSTICS_WAIT });
  const answer = items === null
    ? `The language server said nothing about ${opened.file} within ${DIAGNOSTICS_WAIT} seconds; it may still be checking. Ask again shortly.`
    : listed([{ file: opened.file, items }]) ?? `No errors or warnings in ${opened.file}.`;
  return withInstall(opened.installed, answer);
}

function question(ctx, input: Input) {
  const query = parse(input);
  const opened = ctx.lsp.open({ path: query.file });
  const lineText = (opened.text.split("\n")[query.line - 1] ?? "").replace(/\r$/, "");
  const { method, params } = asked(query, opened.uri, lineText);
  let answer = requested(ctx, query.file, method, params);
  const then = OPERATIONS[query.operation].then;
  const first = Array.isArray(answer) && answer.length > 0 ? answer[0] : undefined;
  if (then !== undefined && first !== undefined) {
    answer = requested(ctx, query.file, then, { item: first });
  }
  return withInstall(opened.installed, describe(ctx, query.operation, answer));
}

function clipped(text: string) {
  const characters = [...text];
  if (characters.length <= OUTPUT) {
    return text;
  }
  return `${characters.slice(0, OUTPUT).join("")}\n… [${characters.length - OUTPUT} more characters cut]`;
}

export function execute(input: Input, ctx) {
  try {
    if (!ctx.lsp.available()) {
      throw new Error("No language servers are available in this run.");
    }
    const operation = typeof input?.operation === "string" ? input.operation : "";
    const answered = SCOPES.includes(operation) ? diagnose(ctx, operation, input) : question(ctx, input ?? {});
    return clipped(answered);
  } catch (error) {
    return { content: String(error.message ?? error), isError: true };
  }
}
