type Language = "en" | "es" | "pt-BR";

type Field = {
  key: string;
  label: string;
  kind: "text" | "area" | "chips" | "select" | "switch" | "color";
  value: unknown;
  options?: string[];
  more?: boolean;
};

type Body = { label: string; text: string; language: string };

type Opened = { mode: "form" | "code"; language: string; fields: Field[]; body: Body | null };

type Asked = {
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
  folder: string;
  icon: string;
  exports: string[];
  needs: string[];
  creatable: boolean;
  label: (language: Language) => string;
  hint: (language: Language) => string;
  about: string;
  ideas: (language: Language) => string[];
  main: (path: string) => boolean;
  card: (asked: Asked) => { title: string; description: string; avatar?: unknown };
  open: (asked: Asked) => Opened;
  save: (saved: Saved) => string;
  create: (name: string, slug: string) => { path: string; source: string };
};

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

const SHAPES = ["circle", "squircle", "triangle", "cloud", "drop", "pill"];
const MODES = ["auto", "plan"];

const withCurrent = (options: string[], current: string[]): string[] => [...new Set([...options, ...current])];

function agentFields({ exports, paths, language }: Asked): Field[] {
  const spec = record(exports.spec);
  const avatar = record(spec.avatar);
  const tools = [...named(paths, "tools/", flat(2, [".ts", ".js"])), ...named(paths, "tools/", flat(3, [".ts", ".js"])), "mcp__*"];
  const skills = named(paths, "skills/", (path) => path.endsWith("/SKILL.md"));
  const agents = named(paths, "agents/", flat(2, [".ts"]));
  const mode = text(spec.permissionMode);
  return [
    { key: "label", label: pick(language, "Name", "Nombre", "Nome"), kind: "text", value: text(spec.label) || text(spec.name) },
    { key: "description", label: pick(language, "Description", "Descripción", "Descrição"), kind: "area", value: text(spec.description) },
    { key: "tools", label: pick(language, "Tools", "Herramientas", "Ferramentas"), kind: "chips", value: texts(spec.tools), options: withCurrent(tools, texts(spec.tools)) },
    { key: "skills", label: "Skills", kind: "chips", value: texts(spec.skills), options: withCurrent(skills, texts(spec.skills)) },
    { key: "permissionMode", label: pick(language, "Mode", "Modo", "Modo"), kind: "select", value: mode, options: withCurrent(MODES, mode ? [mode] : []) },
    { key: "avatar.color", label: pick(language, "Color", "Color", "Cor"), kind: "color", value: text(avatar.color) },
    { key: "avatar.shape", label: pick(language, "Shape", "Forma", "Forma"), kind: "select", value: text(avatar.shape), options: withCurrent(SHAPES, text(avatar.shape) ? [text(avatar.shape)] : []) },
    { key: "disallowedTools", label: pick(language, "Blocked tools", "Herramientas bloqueadas", "Ferramentas bloqueadas"), kind: "chips", value: texts(spec.disallowedTools), options: withCurrent(tools, texts(spec.disallowedTools)), more: true },
    { key: "observer", label: pick(language, "Observers", "Observadores", "Observadores"), kind: "chips", value: texts(spec.observer), options: withCurrent(agents, texts(spec.observer)), more: true },
    { key: "model", label: pick(language, "Model", "Modelo", "Modelo"), kind: "text", value: text(spec.model), more: true },
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
    if (!block && next > at + 1) return null;
    const inside = lines.slice(at + 1, next).map((line) => line.replace(/^ {2}/, ""));
    values[found[1]] = block ? inside.join(found[2].trim().startsWith(">") ? " " : "\n").trim() : unquoted(found[2]);
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
    const fields: Field[] = [
      { key: "name", label: pick(language, "Name", "Nombre", "Nome"), kind: "text", value: parsed.values.name ?? "" },
      { key: "description", label: pick(language, "When to use it", "Cuándo usarla", "Quando usar"), kind: "area", value: parsed.values.description ?? "" },
      { key: "disable-model-invocation", label: pick(language, "Only when called by name", "Solo cuando se llama por nombre", "Só quando chamada pelo nome"), kind: "switch", value: parsed.values["disable-model-invocation"] === "true" },
    ];
    return { mode: "form", language: "markdown", fields, body: { label: pick(language, "Instructions", "Instrucciones", "Instruções"), text: parsed.body, language: "markdown" } };
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

function steps(source: string): string {
  const at = source.indexOf("steps: [");
  if (at < 0) return "";
  const found = [...source.slice(at).matchAll(/^\s{4}([A-Za-z_]\w*)\(/gm)].map((match) => match[1]);
  return found.join(" → ");
}

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
  create: (_name, slug) => ({
    path: `workflows/${slug}.ts`,
    source: `import { workflow } from "sdk";\nimport { plan, develop } from "sdk/steps";\n\nexport default workflow({\n  name: ${JSON.stringify(slug)},\n  accepts: ["issue", "text"],\n  triggers: [],\n  steps: [\n    plan({ agent: "planner", status: { category: "todo" } }),\n    develop({ agent: "developer", status: { category: "doing" } }),\n  ],\n});\n`,
  }),
};

const flows: Kind = {
  key: "flows",
  about: "a branch flow: TypeScript that declares how branches are named, based and merged",
  ideas: (language) =>
    pick(language, ["Explain this flow", "Base everything on main", "Merge with a merge commit"], ["Explica este flujo", "Basa todo en main", "Fusiona con un merge commit"], ["Explique este fluxo", "Baseie tudo na main", "Faça o merge com merge commit"]),
  folder: "flows/",
  icon: "branch",
  exports: [],
  needs: [],
  creatable: true,
  label: (language) => pick(language, "Branch flows", "Flujos de ramas", "Fluxos de branch"),
  hint: (language) => pick(language, "How branches are named, based and merged", "Cómo se nombran, basan y fusionan las ramas", "Como as branches são nomeadas, baseadas e mescladas"),
  main: flat(2, [".ts"]),
  card: ({ path, source }) => ({ title: stem(path), description: /\bbranch:\s*"([^"]+)"/.exec(source)?.[1] ?? "" }),
  open: ({ path }) => code(path),
  save: ({ source }) => source,
  create: (_name, slug) => ({
    path: `flows/${slug}.ts`,
    source: `import { flow } from "sdk/flow.js";\n\nexport const spec = {\n  branch: "feature/{issue}-{slug}",\n  base: "main",\n  method: "squash",\n};\n\nexport const branch = flow(spec);\n`,
  }),
};

const KINDS: Kind[] = [agents, skills, prompts, workflows, flows];

const kind = (key: string): Kind => KINDS.find((item) => item.key === key) ?? fail(`there is no ${key} section`);

function fail(message: string): never {
  throw new Error(message);
}

export function sections({ language }: { language: Language }) {
  return KINDS.map((item) => ({
    key: item.key,
    folder: item.folder,
    icon: item.icon,
    exports: item.exports,
    needs: item.needs,
    creatable: item.creatable,
    label: item.label(language),
    hint: item.hint(language),
  }));
}

export function listing({ paths }: { paths: string[] }) {
  return KINDS.flatMap((item) =>
    paths
      .filter((path) => path.startsWith(item.folder) && item.main(path))
      .map((path) => {
        const folder = path.endsWith(SKILL) ? path.slice(0, -SKILL.length + 1) : null;
        const files = folder ? paths.filter((other) => other.startsWith(folder) && other !== path) : [];
        return { section: item.key, path, files };
      }),
  );
}

export function card(asked: Asked) {
  return kind(asked.section).card(asked);
}

export function open(asked: Asked) {
  const found = kind(asked.section);
  return { ...found.open(asked), assist: { ideas: found.ideas(asked.language) } };
}

export function save(saved: Saved): { source: string } {
  const opened = kind(saved.section).open(saved);
  if (opened.mode === "code") return { source: saved.source };
  return { source: kind(saved.section).save(saved) };
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

export function create({ section, name }: { section: string; name: string }) {
  const item = kind(section);
  if (!item.creatable) fail(`a ${section} item cannot be created here`);
  const slug = name
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  if (slug === "") fail("the name needs a letter or a digit");
  return item.create(name.trim(), slug);
}
