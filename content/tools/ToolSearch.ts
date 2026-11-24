const FOUND = 5;
const MIN_WORD = 3;
const NAME_WEIGHT = 3;
const LOADED = "Loaded; call them from now on:\n";
const WAITING = "\n\nNot loaded yet: ";

export const spec = {
  name: "ToolSearch",
  input: {
    "additionalProperties": false,
    "properties": {
      "query": {
        "type": "string"
      }
    },
    "required": [
      "query"
    ],
    "type": "object"
  },
  description: `Load the schemas of tools that are listed in a system reminder but not yet callable, then call them. Query \`select:name1,name2\` for tools you know by name, or keywords to find them.`,
};

type Input = { query: string };
type Deferred = { name: string; description: string; parameters: unknown; loaded: boolean };

export function offered(ctx) {
  return ctx.services?.deferred === true;
}

export function effects(_input: Input | null) {
  return [];
}

export function summary(input: Input | null) {
  return typeof input?.query === "string" ? input.query : "";
}

function selected(tools: Deferred[], names: string) {
  const wanted = names.split(",").map((name) => name.trim());
  return tools.filter((tool) => wanted.includes(tool.name));
}

function scored(tools: Deferred[], query: string) {
  const words = query
    .split(/\s+/)
    .map((word) => word.toLowerCase())
    .filter((word) => [...word].length >= MIN_WORD);
  const score = (tool: Deferred) => {
    const name = tool.name.toLowerCase();
    const description = tool.description.toLowerCase();
    return words.reduce(
      (sum, word) => sum + (name.includes(word) ? NAME_WEIGHT : description.includes(word) ? 1 : 0),
      0,
    );
  };
  return tools
    .map((tool, at) => ({ tool, at, score: score(tool) }))
    .filter((found) => found.score > 0)
    .sort((a, b) => b.score - a.score || a.at - b.at)
    .slice(0, FOUND)
    .map((found) => found.tool);
}

export function search(tools: Deferred[], query: string) {
  return query.startsWith("select:") ? selected(tools, query.slice("select:".length)) : scored(tools, query);
}

function report(found: Deferred[], waiting: string[]) {
  const shown = found.map((tool) => ({ name: tool.name, description: tool.description, parameters: tool.parameters }));
  const rest = waiting.length === 0
    ? ""
    : `${WAITING}${waiting.join(", ")}. If none above is the tool you need, load it by exact name with the query "select:<name>[,<name>]" instead of calling a loaded tool that does something else.`;
  return `${LOADED}${JSON.stringify(shown, null, 2)}${rest}`;
}

export function execute(input: Input, ctx) {
  if (typeof input?.query !== "string") {
    return { content: "Missing required parameter `query`.", isError: true };
  }
  const query = input.query;
  const tools: Deferred[] | null = ctx.sessions.deferred();
  if (tools === null) {
    return { content: "Every tool is already loaded.", isError: true };
  }
  const found = search(tools, query);
  if (found.length === 0) {
    return { content: `No tool matches "${query}".`, isError: true };
  }
  const { waiting } = ctx.sessions.load({ names: found.map((tool) => tool.name) });
  return report(found, waiting);
}
