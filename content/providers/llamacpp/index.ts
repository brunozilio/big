import { type Model, capabilities, invalid, option, parsed, pick, text, unauthorized, words } from "sdk/providers/index.ts";

type File = { name: string; size: number | null };
type Wanted = { repo: string; quant: string | null };
type Found = { repo: string; files: File[] };

const SAVED_TOKEN = ".cache/huggingface/token";
const ANONYMOUS = "anonymous";
const PART = /-(\d{5})-of-(\d{5})\.gguf$/i;
const QUANT = /[.\-_]((?:I?Q\d[A-Z0-9_]*)|BF16|F16|F32)(?:-\d{5}-of-\d{5})?\.gguf$/i;
const CANDIDATES = 30;
const TRIED = 6;

const FORMATS = [
  {
    when: "<function=",
    think: { open: "<think>", close: "</think>" },
    call: { open: "<tool_call>", close: "</tool_call>", format: "xml" },
  },
  {
    when: "<tool_call>",
    think: { open: "<think>", close: "</think>" },
    call: { open: "<tool_call>", close: "</tool_call>", format: "json" },
  },
  { when: "", think: { open: "<think>", close: "</think>" } },
];

export function provider() {
  return {
    id: "llamacpp-local",
    name: "llama.cpp",
    label: "llama.cpp",
    logo: "logo.svg",
    vendor: "",
    access: "browser",
    rank: 95,
    slot: "0x7072_6f76_6964_6572_0000_0000_0000_000b",
    window: 65_536,
    wire: "chat",
    path: "/chat/completions",
    servesAll: true,
  };
}

export function fields({ language }: { language: string }) {
  return [
    {
      name: "model",
      label: pick(language, "Hugging Face model", "Modelo de Hugging Face", "Modelo do Hugging Face"),
      kind: "text",
      mono: true,
      placeholder: "huihui-ai/Huihui-Qwen3.8-27B-abliterated",
      help: pick(
        language,
        "owner/repo, optionally with :QUANT (e.g. :Q8_0). A repo without GGUF files is served from its most downloaded GGUF quantization.",
        "dueño/repo, opcionalmente con :QUANT (p. ej. :Q8_0). Un repo sin archivos GGUF usa su cuantización GGUF más descargada.",
        "dono/repo, opcionalmente com :QUANT (ex.: :Q8_0). Um repo sem arquivos GGUF usa a quantização GGUF mais baixada dele.",
      ),
    },
    {
      name: "token",
      label: pick(language, "Hugging Face token", "Token de Hugging Face", "Token do Hugging Face"),
      kind: "text",
      mono: true,
      secret: true,
      optional: true,
      placeholder: "hf_…",
      help: pick(
        language,
        "Like `hf auth login`: create one at huggingface.co/settings/tokens. Left empty, the token saved by the hf CLI is used, or none for public models.",
        "Como `hf auth login`: créalo en huggingface.co/settings/tokens. Vacío, se usa el token guardado por el CLI hf, o ninguno para modelos públicos.",
        "Como no `hf auth login`: crie em huggingface.co/settings/tokens. Vazio, usa o token salvo pelo CLI hf, ou nenhum para modelos públicos.",
      ),
    },
  ];
}

export const base = (_: unknown, ctx) => ctx.local.endpoint();

export const headers = () => [["accept", "text/event-stream"]];

export const catalog = (_: unknown, ctx) => ({ url: `${ctx.local.endpoint()}/models`, headers: [["accept", "application/json"]], body: null });

function shown(id: string): boolean {
  if (!id.toLowerCase().endsWith(".gguf") || /mmproj/i.test(id)) return false;
  const part = id.match(PART);
  return !part || Number(part[1]) === 1;
}

function labelOf(item): string {
  const file = String(item.id).split("/").pop() ?? item.id;
  const name = file.replace(PART, ".gguf").replace(/\.gguf$/i, "");
  if (item.complete) return name;
  const percent = item.total ? Math.floor((100 * item.received) / item.total) : 0;
  return item.error ? `${name} (${item.error})` : `${name} (${percent}%)`;
}

export function page({ value }, ctx) {
  const listed = Array.isArray(value?.data) ? value.data : [];
  const models: Model[] = listed
    .filter((item) => typeof item?.id === "string" && shown(item.id))
    .map((item) => {
      const model = option(ctx, item.id, labelOf(item));
      model.window = Number.parseInt(ctx.config.context, 10) || null;
      model.capabilities = capabilities({ reasoning: true });
      return model;
    });
  return { models, next: null };
}

function effortOf(effort: string): Record<string, unknown> {
  if (effort === "none" || effort === "off" || effort === "minimal") return { enable_thinking: false };
  if (effort === "low" || effort === "medium") return { enable_thinking: true, reasoning_effort: effort };
  if (effort === "") return {};
  return { enable_thinking: true, reasoning_effort: "xhigh" };
}

function number(value: unknown): number | undefined {
  const parsed = Number.parseFloat(String(value ?? ""));
  return Number.isFinite(parsed) ? parsed : undefined;
}

export function shape({ body, effort }, ctx) {
  const config = ctx.config;
  const sampling = { temperature: config.temperature, top_p: config.top_p, top_k: config.top_k, min_p: config.min_p };
  for (const [key, value] of Object.entries(sampling)) {
    if (body[key] === undefined && number(value) !== undefined) body[key] = number(value);
  }
  const layers = number(config.gpu_layers);
  body.local = {
    context: Number.parseInt(config.context, 10) || 0,
    ...(layers === undefined ? {} : { gpu_layers: layers }),
    formats: FORMATS,
    template_kwargs: effortOf(String(effort ?? "")),
  };
  return body;
}

export function wanted(raw: unknown): Wanted {
  const typed = text(raw)?.trim() ?? invalid("say which Hugging Face model to run, as owner/repo");
  const bare = typed.replace(/^https?:\/\/(www\.)?huggingface\.co\//i, "").replace(/\/(tree|blob|resolve)\/.*$/, "").replace(/\/+$/, "");
  const [repo, quant] = bare.split(":");
  if (!/^[\w.-]+\/[\w.-]+$/.test(repo)) invalid(`${typed} is not a Hugging Face model; write it as owner/repo`);
  return { repo, quant: text(quant)?.trim() ?? null };
}

const authorization = (token: string): [string, string][] => (token ? [["authorization", `Bearer ${token}`]] : []);

function savedToken(ctx): string {
  const saved = ctx.files.home(SAVED_TOKEN);
  return typeof saved === "string" ? saved.trim() : "";
}

function whoami(ctx, token: string): string {
  if (!token) return ANONYMOUS;
  const reached = ctx.http.get({ url: `${ctx.config.hub}/api/whoami-v2`, headers: authorization(token) });
  if (reached.status === 401) unauthorized("Hugging Face refused the token");
  if (reached.status !== 200) invalid(`Hugging Face answered HTTP ${reached.status} when checking the token`);
  return text(parsed(reached.body)?.name) ?? ANONYMOUS;
}

function filesOf(ctx, token: string, repo: string): File[] {
  const reached = ctx.http.get({ url: `${ctx.config.hub}/api/models/${repo}?blobs=true`, headers: authorization(token) });
  if (reached.status === 401 || reached.status === 403) {
    unauthorized(`${repo} is gated or private: accept its terms on Hugging Face and sign in with a token that can read it`);
  }
  if (reached.status === 404) invalid(`there is no model ${repo} on Hugging Face`);
  if (reached.status !== 200) invalid(`Hugging Face answered HTTP ${reached.status} for ${repo}`);
  const siblings = parsed(reached.body)?.siblings;
  const all = Array.isArray(siblings) ? siblings : [];
  return all
    .filter((item) => typeof item?.rfilename === "string" && /\.gguf$/i.test(item.rfilename) && !/mmproj/i.test(item.rfilename))
    .map((item) => ({ name: item.rfilename, size: typeof item.size === "number" ? item.size : null }));
}

const quantOf = (name: string) => name.match(QUANT)?.[1]?.toUpperCase() ?? "";

export function chosen(files: File[], quant: string | null, preferred: string[]): File[] {
  const order = quant ? [quant.toUpperCase()] : preferred.map((name) => name.toUpperCase());
  const byName = (a: File, b: File) => (a.name < b.name ? -1 : 1);
  for (const want of order) {
    const matching = files.filter((file) => quantOf(file.name) === want);
    if (matching.length > 0) return matching.sort(byName);
  }
  if (quant || files.length === 0) return [];
  const first = quantOf(files[0].name);
  return files.filter((file) => quantOf(file.name) === first).sort(byName);
}

function derivatives(ctx, token: string, repo: string): string[] {
  const url = `${ctx.config.hub}/api/models?filter=base_model:quantized:${repo}&sort=downloads&direction=-1&limit=${CANDIDATES}`;
  const reached = ctx.http.get({ url, headers: authorization(token) });
  const listed = reached.status === 200 ? parsed(reached.body) : [];
  const ids: string[] = (Array.isArray(listed) ? listed : []).map((item) => item?.id).filter((id) => typeof id === "string");
  const name = repo.split("/")[1].toLowerCase();
  const close = ids.filter((id) => id.split("/")[1]?.toLowerCase().startsWith(name));
  return [...close, ...ids.filter((id) => !close.includes(id))].filter((id) => /gguf/i.test(id) || close.includes(id));
}

export function resolve(ctx, token: string, want: Wanted): Found {
  const preferred = words(ctx.config.quants);
  const own = chosen(filesOf(ctx, token, want.repo), want.quant, preferred);
  if (own.length > 0) return { repo: want.repo, files: own };
  for (const candidate of derivatives(ctx, token, want.repo).slice(0, TRIED)) {
    const files = chosen(filesOf(ctx, token, candidate), want.quant, preferred);
    if (files.length > 0) return { repo: candidate, files };
  }
  const which = want.quant ? ` with ${want.quant}` : "";
  return invalid(`neither ${want.repo} nor its quantizations on Hugging Face have GGUF files${which} that llama.cpp can run`);
}

function fetched(ctx, token: string, found: Found) {
  for (const file of found.files) {
    const path = file.name.split("/").map(encodeURIComponent).join("/");
    ctx.local.download({
      name: `${found.repo}/${file.name}`,
      url: `${ctx.config.hub}/${found.repo}/resolve/main/${path}`,
      headers: authorization(token),
      bytes: file.size,
    });
  }
}

export function signIn({ fields }, ctx) {
  const want = wanted(fields?.model);
  const token = text(fields?.token)?.trim() ?? savedToken(ctx);
  const who = whoami(ctx, token);
  const found = resolve(ctx, token, want);
  fetched(ctx, token, found);
  ctx.log(`downloading ${found.files.map((file) => file.name).join(", ")} from ${found.repo}`);
  return { auth: { access_token: token || ANONYMOUS, account_id: who, email: who } };
}
