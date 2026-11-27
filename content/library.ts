type Language = "en" | "es" | "pt-BR";

type Field = {
  key: string;
  label: string;
  kind: "text" | "area" | "chips" | "select" | "switch" | "color";
  value: unknown;
  options?: string[];
  from?: string;
  more?: boolean;
  required?: boolean;
  max?: number;
  pattern?: string;
  hint?: string;
  readOnly?: boolean;
};

type Body = { label: string; text: string; language: string; maxLines?: number };

type Opened = { mode: "form" | "code"; language: string; fields: Field[]; body: Body | null };

type Host = { call?: (name: string, value: unknown) => unknown };

type Action = { key: string; label: string; hint?: string };

type Asked = Host & {
  section: string;
  path: string;
  source: string;
  exports: Record<string, unknown>;
  paths: string[];
  language: Language;
};

type Saved = Asked & { values: Record<string, unknown>; body: string | null };

type Turn = { role: string; text: string };

type Assisting = Saved & { draft: string | null; messages: Turn[]; answer?: string };

type Kind = {
  key: string;
  folder?: string;
  icon: string;
  exports?: string[];
  needs: string[];
  creatable: boolean;
  label: (language: Language) => string;
  hint: (language: Language) => string;
  about: string;
  ideas: (language: Language) => string[];
  main: (path: string, host: Host) => boolean;
  resolve?: (host: Host) => { folder: string; exports: string[] };
  actions?: (asked: Asked) => Action[];
  act?: (acting: Asked & { action: string }) => { source?: string; path?: string; error?: string };
  card: (asked: Asked) => { title: string; description: string; avatar?: unknown };
  open: (asked: Asked) => Opened;
  save: (saved: Saved) => string | { source: string; path: string };
  create: (name: string, slug: string, host: Host) => { path: string; source: string };
  outline?: (source: string) => Step[];
};

type Step = { line: number; name: string; agent: string | null; notes: string[] };

function pick<T>(language: Language | string | undefined, english: T, spanish: T, portuguese: T): T {
  if (language === "en") return english;
  if (language === "es") return spanish;
  return portuguese;
}

const stem = (path: string): string => (path.split("/").pop() ?? path).replace(/\.[a-z]+$/, "");

const flat = (depth: number, extensions: string[]) => (path: string) =>
  path.split("/").length === depth && extensions.some((extension) => path.endsWith(extension));

const languageOf = (path: string): string =>
  path.endsWith(".md") ? "markdown" : path.endsWith(".ts") ? "typescript" : path.endsWith(".json") ? "json" : "javascript";

const code = (path: string): Opened => ({ mode: "code", language: languageOf(path), fields: [], body: null });

const texts = (value: unknown): string[] => (Array.isArray(value) ? value.filter((item) => typeof item === "string") : []);

const text = (value: unknown): string => (typeof value === "string" ? value : "");

const record = (value: unknown): Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : {};

function literal(value: unknown, indent: string): string {
  const inner = `${indent}  `;
  if (Array.isArray(value)) {
    if (value.length === 0) return "[]";
    return `[\n${value.map((item) => `${inner}${literal(item, inner)},\n`).join("")}${indent}]`;
  }
  if (value !== null && typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>).filter(([, item]) => item !== undefined);
    if (entries.length === 0) return "{}";
    const named = (key: string) => (/^[A-Za-z_$][\w$]*$/.test(key) ? key : JSON.stringify(key));
    return `{\n${entries.map(([key, item]) => `${inner}${named(key)}: ${literal(item, inner)},\n`).join("")}${indent}}`;
  }
  return JSON.stringify(value);
}

const template = (body: string): string =>
  `\`${body.replace(/\\/g, "\\\\").replace(/`/g, "\\`").replace(/\$\{/g, "\\${")}\``;

const TEMPLATE_ONLY = /^\s*export default `(?:[^`\\]|\\[\s\S])*`;?\s*$/;
const SPEC_AND_TEMPLATE = /^\s*export const spec = \{[\s\S]*?\n\};\s*export default `(?:[^`\\]|\\[\s\S])*`;?\s*$/;

function templated(source: string, shape: RegExp): boolean {
  if (!shape.test(source)) return false;
  const body = source.slice(source.indexOf("export default `"));
  return !/(^|[^\\])\$\{/.test(body);
}

const named = (paths: string[], folder: string, main: (path: string) => boolean): string[] =>
  paths.filter((path) => path.startsWith(folder) && main(path)).map((path) => (path.endsWith("/SKILL.md") ? path.split("/")[1] : stem(path)));

const SHAPES = ["circle", "squircle", "triangle", "cloud", "drop", "pill", "hexagon"];
const MODES = ["auto", "plan"];
const NAME_MAX = 64;
const DESCRIPTION_MAX = 1024;
const COMPATIBILITY_MAX = 500;
const BODY_LINES = 500;
const SKILL_NAME = "^[a-z0-9]+(-[a-z0-9]+)*$";

const withCurrent = (options: string[], current: string[]): string[] => [...new Set([...options, ...current])];

function agentFields({ exports, paths, language }: Asked): Field[] {
  const spec = record(exports.spec);
  const avatar = record(spec.avatar);
  const tools = [...named(paths, "tools/", flat(2, [".ts", ".js"])), ...named(paths, "tools/", flat(3, [".ts", ".js"])), "mcp__*"];
  const skills = named(paths, "skills/", (path) => path.endsWith("/SKILL.md"));
  const agents = named(paths, "agents/", flat(2, [".ts"]));
  const mode = text(spec.permissionMode);
  return [
    { key: "label", label: pick(language, "Name", "Nombre", "Nome"), kind: "text", value: text(spec.label) || text(spec.name), required: true, max: NAME_MAX },
    { key: "description", label: pick(language, "Description", "Descripción", "Descrição"), kind: "area", value: text(spec.description), max: DESCRIPTION_MAX },
    { key: "tools", label: pick(language, "Tools", "Herramientas", "Ferramentas"), kind: "chips", value: texts(spec.tools), options: withCurrent(tools, texts(spec.tools)) },
    { key: "skills", label: "Skills", kind: "chips", value: texts(spec.skills), options: withCurrent(skills, texts(spec.skills)) },
    { key: "permissionMode", label: pick(language, "Mode", "Modo", "Modo"), kind: "select", value: mode, options: withCurrent(MODES, mode ? [mode] : []) },
    { key: "avatar.color", label: pick(language, "Color", "Color", "Cor"), kind: "color", value: text(avatar.color) },
    { key: "avatar.shape", label: pick(language, "Shape", "Forma", "Forma"), kind: "select", value: text(avatar.shape), options: withCurrent(SHAPES, text(avatar.shape) ? [text(avatar.shape)] : []) },
    { key: "disallowedTools", label: pick(language, "Blocked tools", "Herramientas bloqueadas", "Ferramentas bloqueadas"), kind: "chips", value: texts(spec.disallowedTools), options: withCurrent(tools, texts(spec.disallowedTools)), more: true },
    { key: "observer", label: pick(language, "Observers", "Observadores", "Observadores"), kind: "chips", value: texts(spec.observer), options: withCurrent(agents, texts(spec.observer)), more: true },
    { key: "model", label: pick(language, "Model", "Modelo", "Modelo"), kind: "select", value: text(spec.model), options: withCurrent(["", "inherit"], [text(spec.model)]), from: "models", more: true },
  ];
}

function emptied(value: unknown): boolean {
  return value === "" || value === null || value === undefined || (Array.isArray(value) && value.length === 0);
}

function agentSource({ exports, values, body }: Saved): string {
  const spec: Record<string, unknown> = { ...record(exports.spec) };
  for (const [key, value] of Object.entries(values)) {
    const [outer, inner] = key.split(".");
    if (inner === undefined) {
      if (emptied(value) && !(key in spec)) continue;
      if (emptied(value) && typeof value === "string") delete spec[key];
      else spec[key] = value;
      continue;
    }
    if (emptied(value)) continue;
    spec[outer] = { ...record(spec[outer]), [inner]: value };
  }
  return `export const spec = ${literal(spec, "")};\n\nexport default ${template(body ?? text(exports.default))};\n`;
}

const agents: Kind = {
  key: "agents",
  about: "an agent: its settings and the system prompt it runs with",
  ideas: (language) =>
    pick(language, ["Make the prompt shorter", "Add a step that runs the tests", "Review this prompt for ambiguity"], ["Acorta el prompt", "Añade un paso que ejecute las pruebas", "Revisa este prompt en busca de ambigüedades"], ["Deixe o prompt mais curto", "Acrescente um passo que roda os testes", "Revise este prompt e aponte ambiguidades"]),
  folder: "agents/",
  icon: "bot",
  exports: ["spec", "default"],
  needs: ["tools/", "skills/"],
  creatable: true,
  label: (language) => pick(language, "Agents", "Agentes", "Agentes"),
  hint: (language) => pick(language, "Who does the work in each step", "Quién hace el trabajo en cada etapa", "Quem faz o trabalho em cada etapa"),
  main: flat(2, [".ts"]),
  card({ path, exports }) {
    const spec = record(exports.spec);
    return { title: text(spec.label) || text(spec.name) || stem(path), description: text(spec.description), avatar: spec.avatar ?? null };
  },
  open(asked) {
    const fits = templated(asked.source, SPEC_AND_TEMPLATE) && typeof asked.exports.default === "string" && asked.exports.spec != null;
    if (!fits) return code(asked.path);
    const body = { label: "Prompt", text: text(asked.exports.default), language: "markdown" };
    return { mode: "form", language: "typescript", fields: agentFields(asked), body };
  },
  save: agentSource,
  create(name, slug) {
    const spec = { name: slug, label: name, description: "", tools: ["Read", "Grep", "Glob"], permissionMode: "auto" };
    return { path: `agents/${slug}.ts`, source: `export const spec = ${literal(spec, "")};\n\nexport default ${template(`You are ${name}.`)};\n` };
  },
};

type Matter = { lines: string[]; values: Record<string, string>; spans: Record<string, [number, number]>; body: string };

function unquoted(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length > 1 && trimmed.startsWith('"') && trimmed.endsWith('"')) {
    try {
      return JSON.parse(trimmed);
    } catch {
      return trimmed.slice(1, -1);
    }
  }
  if (trimmed.length > 1 && trimmed.startsWith("'") && trimmed.endsWith("'")) return trimmed.slice(1, -1).replace(/''/g, "'");
  return trimmed;
}

function matter(source: string): Matter | null {
  const all = source.split("\n");
  if (all[0]?.trim() !== "---") return null;
  const end = all.indexOf("---", 1);
  if (end < 0) return null;
  const lines = all.slice(1, end);
  const values: Record<string, string> = {};
  const spans: Record<string, [number, number]> = {};
  let at = 0;
  while (at < lines.length) {
    const found = /^([A-Za-z][\w-]*):\s?(.*)$/.exec(lines[at]);
    if (!found) return null;
    let next = at + 1;
    while (next < lines.length && (lines[next].startsWith(" ") || lines[next].trim() === "")) next += 1;
    const block = /^[|>][+-]?$/.test(found[2].trim());
    const nested = found[2].trim() === "" && next > at + 1;
    if (!block && !nested && next > at + 1) return null;
    const inside = lines.slice(at + 1, next).map((line) => line.replace(/^ {2}/, ""));
    values[found[1]] = nested ? "" : block ? inside.join(found[2].trim().startsWith(">") ? " " : "\n").trim() : unquoted(found[2]);
    spans[found[1]] = [at, next];
    at = next;
  }
  return { lines, values, spans, body: all.slice(end + 1).join("\n").replace(/^\n/, "") };
}

function scalar(key: string, value: string): string[] {
  const plain = !value.includes("\n") && !/: |\s#|^[\s"'|>&*!%@`\[{-]|\s$/.test(value) && value !== "";
  if (plain) return [`${key}: ${value}`];
  if (!value.includes("\n")) return [`${key}: ${JSON.stringify(value)}`];
  return [`${key}: |-`, ...value.split("\n").map((line) => (line === "" ? "" : `  ${line}`))];
}

function skillSource({ source, values, body }: Saved): string {
  const parsed = matter(source);
  if (!parsed) return source;
  const lines: (string | null)[] = [...parsed.lines];
  const added: string[] = [];
  for (const [key, value] of Object.entries(values)) {
    const wanted = typeof value === "boolean" ? (value ? "true" : "") : text(value);
    if (wanted === (parsed.values[key] ?? "")) continue;
    const written = wanted === "" ? [] : scalar(key, wanted);
    const span = parsed.spans[key];
    if (!span) {
      added.push(...written);
      continue;
    }
    for (let at = span[0]; at < span[1]; at += 1) lines[at] = null;
    if (written.length > 0) lines[span[0]] = written.join("\n");
  }
  const head = [...lines.filter((line): line is string => line !== null), ...added];
  return `---\n${head.join("\n")}\n---\n\n${(body ?? parsed.body).replace(/\n*$/, "")}\n`;
}

const SKILL = "/SKILL.md";

const skills: Kind = {
  key: "skills",
  about: "a skill: when to use it and the instructions an agent loads when it does",
  ideas: (language) =>
    pick(language, ["Make the description say when to use it", "Add an example", "Tighten the instructions"], ["Haz que la descripción diga cuándo usarla", "Añade un ejemplo", "Ajusta las instrucciones"], ["Faça a descrição dizer quando usar", "Acrescente um exemplo", "Enxugue as instruções"]),
  folder: "skills/",
  icon: "idea",
  exports: [],
  needs: [],
  creatable: true,
  label: () => "Skills",
  hint: (language) => pick(language, "Know-how the agents load when they need it", "Conocimiento que los agentes cargan cuando lo necesitan", "Conhecimento que os agentes carregam quando precisam"),
  main: (path) => path.split("/").length === 3 && path.endsWith(SKILL),
  card({ path, source }) {
    const parsed = matter(source);
    return { title: parsed?.values.name || path.split("/")[1], description: parsed?.values.description ?? "" };
  },
  open(asked) {
    const parsed = asked.path.endsWith(SKILL) ? matter(asked.source) : null;
    if (!parsed) return code(asked.path);
    const { language } = asked;
    const named = pick(language, "Lowercase letters, digits and single hyphens", "Minúsculas, dígitos y guiones simples", "Minúsculas, dígitos e hífens simples");
    const folder = asked.path.split("/")[1];
    const fields: Field[] = [
      { key: "name", label: pick(language, "Name", "Nombre", "Nome"), kind: "text", value: parsed.values.name ?? "", required: true, max: NAME_MAX, pattern: SKILL_NAME, hint: named, readOnly: parsed.values.name === folder },
      { key: "description", label: pick(language, "When to use it", "Cuándo usarla", "Quando usar"), kind: "area", value: parsed.values.description ?? "", required: true, max: DESCRIPTION_MAX },
      { key: "disable-model-invocation", label: pick(language, "Only when called by name", "Solo cuando se llama por nombre", "Só quando chamada pelo nome"), kind: "switch", value: parsed.values["disable-model-invocation"] === "true" },
      { key: "compatibility", label: pick(language, "Compatibility", "Compatibilidad", "Compatibilidade"), kind: "area", value: parsed.values.compatibility ?? "", max: COMPATIBILITY_MAX, more: true },
      { key: "allowed-tools", label: pick(language, "Pre-approved tools", "Herramientas preaprobadas", "Ferramentas pré-aprovadas"), kind: "text", value: parsed.values["allowed-tools"] ?? "", more: true },
      { key: "license", label: pick(language, "License", "Licencia", "Licença"), kind: "text", value: parsed.values.license ?? "", more: true },
    ];
    return { mode: "form", language: "markdown", fields, body: { label: pick(language, "Instructions", "Instrucciones", "Instruções"), text: parsed.body, language: "markdown", maxLines: BODY_LINES } };
  },
  save: skillSource,
  create: (name, slug) => ({ path: `skills/${slug}${SKILL}`, source: `---\nname: ${slug}\n${scalar("description", name).join("\n")}\n---\n\n# ${name}\n` }),
};

const prompts: Kind = {
  key: "prompts",
  about: "a prompt the system sends to the model",
  ideas: (language) =>
    pick(language, ["Make it shorter", "Review it for contradictions", "Explain what this prompt does"], ["Hazlo más corto", "Revísalo en busca de contradicciones", "Explica qué hace este prompt"], ["Deixe mais curto", "Revise e aponte contradições", "Explique o que este prompt faz"]),
  folder: "prompts/",
  icon: "note",
  exports: ["default"],
  needs: [],
  creatable: false,
  label: () => "Prompts",
  hint: (language) => pick(language, "The texts the system sends to the model", "Los textos que el sistema envía al modelo", "Os textos que o sistema envia ao modelo"),
  main: flat(2, [".js", ".md"]),
  card: ({ path }) => ({ title: stem(path), description: "" }),
  open(asked) {
    const label = "Prompt";
    if (asked.path.endsWith(".md")) return { mode: "form", language: "markdown", fields: [], body: { label, text: asked.source, language: "markdown" } };
    if (!templated(asked.source, TEMPLATE_ONLY) || typeof asked.exports.default !== "string") return code(asked.path);
    return { mode: "form", language: "javascript", fields: [], body: { label, text: asked.exports.default, language: "markdown" } };
  },
  save: ({ path, source, body }) => (body === null ? source : path.endsWith(".md") ? `${body.replace(/\n*$/, "")}\n` : `export default ${template(body)};\n`),
  create: (_name, slug) => ({ path: `prompts/${slug}.md`, source: "" }),
};

function outlined(source: string): Step[] {
  const lines = source.split("\n");
  const from = lines.findIndex((line) => /^\s*steps:\s*\[\s*$/.test(line));
  if (from < 0) return [];
  const found: Step[] = [];
  for (let at = from + 1; at < lines.length && !/^\s*\],?\s*$/.test(lines[at]); at += 1) {
    const call = /^\s*([A-Za-z_]\w*)\((.*)\),?\s*$/.exec(lines[at]);
    if (!call) continue;
    const back = /\bon(?:Fail|Changes):\s*"([^"]+)"/.exec(call[2])?.[1];
    const retries = /\bretries:\s*(\d+)/.exec(call[2])?.[1];
    const notes = back ? [`↩ ${back}${retries ? ` ×${retries}` : ""}`] : [];
    found.push({ line: at + 1, name: /\bid:\s*"([^"]+)"/.exec(call[2])?.[1] ?? call[1], agent: /\bagent:\s*"([^"]+)"/.exec(call[2])?.[1] ?? null, notes });
  }
  return found;
}

const steps = (source: string): string => outlined(source).map((step) => step.name).join(" → ");

const workflows: Kind = {
  key: "workflows",
  about: "a workflow: TypeScript that composes the steps an item goes through, in order",
  ideas: (language) =>
    pick(language, ["Explain this workflow", "Remove the security step", "Retry the checks five times"], ["Explica este workflow", "Quita la etapa de seguridad", "Reintenta las verificaciones cinco veces"], ["Explique este workflow", "Remova a etapa de segurança", "Repita as verificações cinco vezes"]),
  folder: "workflows/",
  icon: "workflow",
  exports: [],
  needs: [],
  creatable: true,
  label: () => "Workflows",
  hint: (language) => pick(language, "The steps an item goes through", "Las etapas que recorre un ítem", "As etapas que um item percorre"),
  main: flat(2, [".ts"]),
  card: ({ path, source }) => ({ title: /\bname:\s*"([^"]+)"/.exec(source)?.[1] ?? stem(path), description: steps(source) }),
  open: ({ path }) => code(path),
  save: ({ source }) => source,
  outline: outlined,
  create: (_name, slug) => ({
    path: `workflows/${slug}.ts`,
    source: `import { workflow } from "sdk";\nimport { plan, develop } from "sdk/steps";\n\nexport default workflow({\n  name: ${JSON.stringify(slug)},\n  accepts: ["issue", "text"],\n  triggers: [],\n  steps: [\n    plan({ agent: "planner", status: { category: "todo" } }),\n    develop({ agent: "developer", status: { category: "doing" } }),\n  ],\n});\n`,
  }),
};

const brief = (value: string): string => {
  const first = value.split(/(?<=\.)\s/)[0] ?? "";
  return first.length > 180 ? `${first.slice(0, 177)}…` : first;
};

const tools: Kind = {
  key: "tools",
  about: "a tool the agents can call: TypeScript that declares its input and runs it",
  ideas: (language) =>
    pick(language, ["Explain what this tool does", "Make the description clearer", "Review the input for missing checks"], ["Explica qué hace esta herramienta", "Aclara la descripción", "Revisa la entrada en busca de validaciones faltantes"], ["Explique o que esta ferramenta faz", "Deixe a descrição mais clara", "Revise a entrada e aponte validações que faltam"]),
  folder: "tools/",
  icon: "terminal",
  exports: ["spec"],
  needs: [],
  creatable: false,
  label: (language) => pick(language, "Tools", "Herramientas", "Ferramentas"),
  hint: (language) => pick(language, "What the agents can do", "Lo que los agentes pueden hacer", "O que os agentes conseguem fazer"),
  main: (path) => flat(2, [".ts", ".js"])(path) || flat(3, [".ts", ".js"])(path),
  card: ({ path, exports }) => ({ title: text(record(exports.spec).name) || stem(path), description: brief(text(record(exports.spec).description)) }),
  open: ({ path }) => code(path),
  save: ({ source }) => source,
  create: (_name, slug) => ({ path: `tools/${slug}.ts`, source: "" }),
};

const styles: Kind = {
  key: "styles",
  about: "an output style: how the agent writes its replies",
  ideas: (language) =>
    pick(language, ["Make it more direct", "Add a rule about code blocks", "Explain what this style changes"], ["Hazlo más directo", "Añade una regla sobre bloques de código", "Explica qué cambia este estilo"], ["Deixe mais direto", "Acrescente uma regra sobre blocos de código", "Explique o que este estilo muda"]),
  folder: "output-styles/",
  icon: "textStyle",
  exports: [],
  needs: [],
  creatable: true,
  label: (language) => pick(language, "Reply styles", "Estilos de respuesta", "Estilos de resposta"),
  hint: (language) => pick(language, "How the agent writes in conversations", "Cómo escribe el agente en las conversaciones", "Como o agente escreve nas conversas"),
  main: flat(2, [".md"]),
  card({ path, source }) {
    const parsed = matter(source);
    return { title: parsed?.values.name || stem(path), description: parsed?.values.description ?? "" };
  },
  open(asked) {
    const parsed = matter(asked.source);
    if (!parsed) return code(asked.path);
    const { language } = asked;
    const fields: Field[] = [
      { key: "name", label: pick(language, "Name", "Nombre", "Nome"), kind: "text", value: parsed.values.name ?? "", required: true, max: NAME_MAX },
      { key: "description", label: pick(language, "Description", "Descripción", "Descrição"), kind: "area", value: parsed.values.description ?? "", max: DESCRIPTION_MAX },
      { key: "turn-reminder", label: pick(language, "Reminder on each turn", "Recordatorio en cada turno", "Lembrete a cada turno"), kind: "area", value: parsed.values["turn-reminder"] ?? "", max: COMPATIBILITY_MAX },
      { key: "keep-coding-instructions", label: pick(language, "Keep the coding instructions", "Mantener las instrucciones de código", "Manter as instruções de código"), kind: "switch", value: parsed.values["keep-coding-instructions"] === "true" },
    ];
    return { mode: "form", language: "markdown", fields, body: { label: pick(language, "Instructions", "Instrucciones", "Instruções"), text: parsed.body, language: "markdown" } };
  },
  save: skillSource,
  create: (name, slug) => ({ path: `output-styles/${slug}.md`, source: `---\n${scalar("name", name).join("\n")}\ndescription: ""\n---\n\n` }),
};


const BANK = "repl/bank.ts";

type Format = { bank: string; proposed: string; extensions: string[]; spec: string };

function formatOf(host: Host): Format {
  const declared = record(host.call?.("value", { path: BANK, name: "format" }));
  const extensions = texts(declared.extensions);
  return { bank: text(declared.bank), proposed: text(declared.proposed), extensions, spec: text(declared.spec) };
}

const within = (folder: string, extensions: string[], path: string): boolean =>
  folder !== "" && path.startsWith(folder) && !path.slice(folder.length).includes("/") && extensions.some((extension) => path.endsWith(extension));

const proposed = (path: string, host: Host): boolean => {
  const format = formatOf(host);
  return within(format.proposed, format.extensions, path);
};

function scriptTemplate(name: string, slug: string): string {
  const spec = {
    name: slug,
    description: name,
    input: { type: "object", properties: { path: { type: "string", description: "Folder to work in." } }, additionalProperties: false },
    tools: ["Glob", "Read"],
  };
  const body = `type Input = { path?: string };\n\nexport async function run(input: Input = {}) {\n  const found = await Glob({ pattern: "**/*", path: input.path });\n  return { files: found.numFiles ?? 0 };\n}\n`;
  return `export const spec = ${literal(spec, "")};\n\n${body}`;
}

const scripts: Kind = {
  key: "scripts",
  about: "a script of the bank: TypeScript that exports spec (name, description, input as a JSON schema, the tools it calls) and an async run(input); agents find it with searchScripts and run it from the REPL as scripts[\"name\"](input), calling their tools as global functions (Grep, Read, $`cmd`, file(path)) instead of redoing the flow turn by turn",
  ideas: (language) =>
    pick(language, ["Explain what this script does", "Make the input path optional", "Return less data"], ["Explica qué hace este script", "Haz opcional la ruta de entrada", "Devuelve menos datos"], ["Explique o que este script faz", "Torne o caminho de entrada opcional", "Devolva menos dados"]),
  resolve: (host) => {
    const format = formatOf(host);
    return { folder: format.bank, exports: [format.spec].filter((name) => name !== "") };
  },
  icon: "code",
  needs: [],
  creatable: true,
  label: () => "Scripts",
  hint: (language) => pick(language, "Ready-made flows the agents run instead of redoing them", "Flujos listos que los agentes ejecutan en vez de rehacerlos", "Fluxos prontos que os agentes rodam em vez de refazer"),
  main: (path, host) => {
    const format = formatOf(host);
    return within(format.bank, format.extensions, path) || proposed(path, host);
  },
  card(asked) {
    const { path, exports, language } = asked;
    const spec = record(exports[formatOf(asked).spec]);
    const waiting = proposed(path, asked) ? pick(language, "Proposed from a run, waiting for approval. ", "Propuesto a partir de una ejecución, esperando aprobación. ", "Proposto a partir de uma execução, aguardando aprovação. ") : "";
    return { title: text(spec.name) || stem(path), description: `${waiting}${text(spec.description)}` };
  },
  open: (asked) => code(asked.path),
  actions: (asked) =>
    proposed(asked.path, asked)
      ? [{ key: "approve", label: pick(asked.language, "Approve", "Aprobar", "Aprovar"), hint: pick(asked.language, "Agents may run it: it moves into the bank", "Los agentes pueden ejecutarlo: pasa al banco", "Os agentes podem rodá-lo: entra no banco") }]
      : [],
  act(acting) {
    if (acting.action !== "approve" || !proposed(acting.path, acting)) return { error: `${acting.path} has no ${acting.action} action` };
    return { source: acting.source, path: `${formatOf(acting).bank}${acting.path.split("/").pop()}` };
  },
  save: ({ source }) => source,
  create: (name, slug, host) => {
    const format = formatOf(host);
    return { path: `${format.bank}${slug}${format.extensions[0] ?? ""}`, source: scriptTemplate(name, slug) };
  },
};

const KINDS: Kind[] = [agents, skills, prompts, workflows, tools, styles, scripts];

const kind = (key: string): Kind => KINDS.find((item) => item.key === key) ?? fail(`there is no ${key} section`);

function fail(message: string): never {
  throw new Error(message);
}

const resolved = (item: Kind, host: Host) => ({ folder: item.folder ?? "", exports: item.exports ?? [], ...item.resolve?.(host) });

export function sections(host: Host & { language: Language }) {
  const { language } = host;
  return KINDS.map((item) => ({
    key: item.key,
    ...resolved(item, host),
    icon: item.icon,
    needs: item.needs,
    creatable: item.creatable,
    label: item.label(language),
    hint: item.hint(language),
  }));
}

export function listing(host: Host & { paths: string[] }) {
  return KINDS.flatMap((item) =>
    host.paths
      .filter((path) => path.startsWith(resolved(item, host).folder) && item.main(path, host))
      .map((path) => {
        const folder = path.endsWith(SKILL) ? path.slice(0, -SKILL.length + 1) : null;
        const files = folder ? host.paths.filter((other) => other.startsWith(folder) && other !== path) : [];
        return { section: item.key, path, files };
      }),
  );
}

export function card(asked: Asked) {
  return kind(asked.section).card(asked);
}

export function open(asked: Asked) {
  const found = kind(asked.section);
  return { ...found.open(asked), assist: { ideas: found.ideas(asked.language) }, outline: found.outline?.(asked.source) ?? null, actions: found.actions?.(asked) ?? [] };
}

export function act(acting: Asked & { action: string }) {
  const found = kind(acting.section);
  const offered = (found.actions?.(acting) ?? []).some((action) => action.key === acting.action);
  if (!offered || !found.act) return { error: `${acting.path} has no ${acting.action} action` };
  return found.act(acting);
}

export function outline({ section, source }: { section: string; source: string }) {
  return { steps: kind(section).outline?.(source) ?? [] };
}

function broken(field: Field, value: unknown, language: Language): string | null {
  if (typeof value !== "string" || value === field.value) return null;
  const size = [...value].length;
  if (field.required && value.trim() === "") return pick(language, `${field.label} cannot be empty.`, `${field.label} no puede quedar vacío.`, `${field.label} não pode ficar vazio.`);
  if (field.max !== undefined && size > field.max) return pick(language, `${field.label} takes up to ${field.max} characters and has ${size}.`, `${field.label} admite hasta ${field.max} caracteres y tiene ${size}.`, `${field.label} aceita até ${field.max} caracteres e tem ${size}.`);
  if (field.pattern && value !== "" && !new RegExp(field.pattern).test(value)) return `${field.label}: ${field.hint ?? field.pattern}`;
  return null;
}

export function save(saved: Saved): { source?: string; path?: string; error?: string } {
  const opened = kind(saved.section).open(saved);
  if (opened.mode === "code") return { source: saved.source };
  for (const field of opened.fields) {
    const error = broken(field, saved.values?.[field.key], saved.language);
    if (error) return { error };
  }
  const made = kind(saved.section).save(saved);
  return typeof made === "string" ? { source: made } : made;
}

function request(asking: Assisting): { opened: Opened; text: string } {
  const opened = kind(asking.section).open(asking);
  const text = opened.mode === "form" ? (asking.body ?? opened.body?.text ?? "") : (asking.draft ?? asking.source);
  return { opened, text };
}

const RULES = `Reply with one JSON object and nothing else:
{"reply": string, "values": object, "edits": [{"find": string, "replace": string}], "text": string or null}
- "reply": a short message for the person, in the language they write in, saying what you changed or answering what they asked. Plain sentences, no headings.
- "values": only the fields you change, by key. A chips field takes an array (pick from its options when it lists them), a switch takes true or false, a select takes one of its options. Leave it {} when no field changes.
- "edits": targeted replacements in the text. Each "find" is copied exactly from the current text and occurs exactly once in it. Prefer edits when the change is small next to the text.
- "text": the whole new text, only when you rewrite most of it or it is short; otherwise null. Never send both "text" and "edits".
- When the person only asks a question or wants a review, change nothing: answer in "reply" with {} and [] and null.
- Keep placeholders such as {{NAME}} exactly as written, and keep the language the text is written in unless asked to translate.`;

export function assist(asking: Assisting): { prompt: string } {
  const { opened, text } = request(asking);
  const fields = opened.fields.map((field) => ({ key: field.key, label: field.label, kind: field.kind, options: field.options, value: asking.values?.[field.key] ?? field.value }));
  const talk = asking.messages.map((turn) => `<${turn.role === "assistant" ? "you" : "person"}>\n${turn.text.trim()}\n</${turn.role === "assistant" ? "you" : "person"}>`).join("\n");
  const parts = [
    `You edit one item of the library a person keeps for their AI agents. This item is ${kind(asking.section).about}. The person talks to you in a chat beside the editor, and what you change lands in an unsaved draft they review before saving.`,
    `<item path="${asking.path}" />`,
    fields.length > 0 ? `<fields>\n${JSON.stringify(fields, null, 2)}\n</fields>` : "",
    `<text name="${opened.body?.label ?? asking.path}" language="${opened.body?.language ?? opened.language}">\n${text}\n</text>`,
    `<conversation>\n${talk}\n</conversation>`,
    `Answer the person's last message.\n\n${RULES}`,
  ];
  return { prompt: parts.filter((part) => part !== "").join("\n\n") };
}

function parsed(answer: string): Record<string, unknown> {
  const from = answer.indexOf("{");
  const to = answer.lastIndexOf("}");
  if (from >= 0 && to > from) {
    try {
      return record(JSON.parse(answer.slice(from, to + 1)));
    } catch {
      return { reply: answer };
    }
  }
  return { reply: answer };
}

function fitting(field: Field, value: unknown): unknown {
  if (field.kind === "chips") return Array.isArray(value) ? texts(value) : undefined;
  if (field.kind === "switch") return typeof value === "boolean" ? value : undefined;
  if (field.kind === "select") return typeof value === "string" && (field.options ?? []).includes(value) ? value : undefined;
  return typeof value === "string" ? value : undefined;
}

function edited(whole: string, edits: unknown): { text: string; missed: number } {
  let missed = 0;
  let current = whole;
  for (const edit of Array.isArray(edits) ? edits : []) {
    const find = text(record(edit).find);
    const found = find === "" ? -1 : current.indexOf(find);
    if (found < 0 || current.indexOf(find, found + 1) >= 0) {
      missed += 1;
      continue;
    }
    current = current.slice(0, found) + text(record(edit).replace) + current.slice(found + find.length);
  }
  return { text: current, missed };
}

export function saying({ answer }: { answer: string }): { reply: string } {
  const found = /"reply"\s*:\s*"((?:[^"\\]|\\.)*)/.exec(answer);
  if (!found) return { reply: /^\s*[{`]/.test(answer) || answer.trim() === "" ? "" : answer };
  try {
    return { reply: JSON.parse(`"${found[1].replace(/\\u[0-9a-fA-F]{0,3}$/, "")}"`) };
  } catch {
    return { reply: "" };
  }
}

export function assisted(asking: Assisting) {
  const { opened, text: current } = request(asking);
  const answer = parsed(asking.answer ?? "");
  const values: Record<string, unknown> = {};
  for (const field of opened.fields) {
    const wanted = fitting(field, record(answer.values)[field.key]);
    if (wanted !== undefined) values[field.key] = wanted;
  }
  const whole = typeof answer.text === "string" ? answer.text : null;
  const { text: next, missed } = whole === null ? edited(current, answer.edits) : { text: whole, missed: 0 };
  const note = missed === 0 ? "" : pick(asking.language, ` (${missed} change(s) could not be placed in the text.)`, ` (${missed} cambio(s) no se pudieron ubicar en el texto.)`, ` (${missed} alteração(ões) não puderam ser localizadas no texto.)`);
  const changed = next !== current ? next : null;
  return {
    reply: `${text(answer.reply).trim()}${note}`.trim(),
    values,
    body: opened.mode === "form" ? changed : null,
    source: opened.mode === "code" ? changed : null,
  };
}

export function create(host: Host & { section: string; name: string }) {
  const { section, name } = host;
  const item = kind(section);
  if (!item.creatable) fail(`a ${section} item cannot be created here`);
  const slug = name
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  if (slug === "") fail("the name needs a letter or a digit");
  return item.create(name.trim(), slug, host);
}
