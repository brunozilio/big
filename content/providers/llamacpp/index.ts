import {
  type Config,
  type Model,
  type Reached,
  type Tokens,
  capabilities,
  formRequest,
  invalid,
  option,
  parsed,
  pick,
  standard,
  text,
  tryPost,
  unauthorized,
  words,
} from "sdk/providers/index.ts";

type File = { name: string; size: number | null };
type Wanted = { repo: string; quant: string | null };
type Found = { repo: string; files: File[] };

const GRANT = "urn:ietf:params:oauth:grant-type:device_code";
const MAX_INTERVAL = 60;
const FORM: [string, string][] = [
  ["content-type", "application/x-www-form-urlencoded"],
  ["accept", "application/json"],
];
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
    access: "code",
    rank: 95,
    slot: "0x7072_6f76_6964_6572_0000_0000_0000_000b",
    window: 65_536,
    wire: "chat",
    path: "/chat/completions",
    servesAll: true,
    needs: ["local"],
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

function whoami(ctx, token: string): string {
  const reached = ctx.http.get({ url: `${ctx.config.hub}/api/whoami-v2`, headers: authorization(token) });
  if (reached.status === 401) unauthorized("Hugging Face refused the sign-in");
  if (reached.status !== 200) invalid(`Hugging Face answered HTTP ${reached.status} when checking the sign-in`);
  return text(parsed(reached.body)?.name) ?? invalid("Hugging Face did not say who signed in");
}

export function check({ config }: { config: Config }) {
  if (!config.auth?.access_token) unauthorized("sign in to Hugging Face again");
}

export function refresh({ previous }: { previous: Tokens }, ctx) {
  if (!previous.refresh_token) return null;
  const pairs: [string, string][] = [
    ["grant_type", "refresh_token"],
    ["refresh_token", previous.refresh_token ?? ""],
    ["client_id", ctx.config.client_id],
  ];
  return formRequest(ctx, `${ctx.config.hub}/oauth/token`, pairs);
}

export const tokens = (input, ctx) => standard(ctx, input);

function device(ctx, reached: Reached) {
  const failed = () => unauthorized(`Hugging Face refused the device sign-in (HTTP ${reached.status})`);
  if (reached.status !== 200) failed();
  const value = parsed(reached.body) ?? failed();
  const open = text(value.verification_uri_complete) ?? text(value.verification_uri) ?? failed();
  if (!open.startsWith("https://")) failed();
  return {
    device_code: text(value.device_code) ?? failed(),
    user_code: text(value.user_code) ?? failed(),
    open,
    interval: Math.max(1, Number.isInteger(value.interval) ? value.interval : 5),
    expires_in: Math.max(60, Number.isInteger(value.expires_in) ? value.expires_in : 900),
  };
}

function polled(reached: Reached): "granted" | "pending" | "slow" {
  const value = parsed(reached.body) ?? {};
  if (reached.status === 200 && text(value.access_token)) return "granted";
  if (value.error === "authorization_pending" || (reached.status >= 500 && !value.error)) return "pending";
  if (value.error === "slow_down") return "slow";
  if (value.error === "access_denied") unauthorized("the Hugging Face sign-in was denied");
  if (value.error === "expired_token") unauthorized("the Hugging Face code expired before it was approved");
  if (typeof value.error === "string") unauthorized(text(value.error_description) ?? value.error);
  return "pending";
}

function approved(ctx, code): Reached {
  const body = ctx.oauth.form([
    ["grant_type", GRANT],
    ["device_code", code.device_code],
    ["client_id", ctx.config.client_id],
  ]);
  let pause = code.interval;
  let waited = 0;
  for (;;) {
    if (waited >= code.expires_in) unauthorized("the Hugging Face code expired before it was approved");
    ctx.clock.sleep(pause);
    waited += pause;
    const reached = tryPost(ctx, `${ctx.config.hub}/oauth/token`, FORM, body);
    if (reached === undefined) continue;
    const state = polled(reached);
    if (state === "granted") return reached;
    if (state === "slow") pause = Math.min(pause + 5, MAX_INTERVAL);
  }
}

function signedIn(ctx): Tokens {
  const body = ctx.oauth.form([["client_id", ctx.config.client_id]]);
  const code = device(ctx, ctx.http.post({ url: `${ctx.config.hub}/oauth/device`, headers: FORM, body }));
  ctx.browser.open({ url: code.open, code: code.user_code });
  const { status, body: granted } = approved(ctx, code);
  return tokens({ status, body: granted, previous: null, now: ctx.clock.now() }, ctx);
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
  const auth = signedIn(ctx);
  const who = whoami(ctx, auth.access_token);
  const found = resolve(ctx, auth.access_token, want);
  fetched(ctx, auth.access_token, found);
  ctx.log(`downloading ${found.files.map((file) => file.name).join(", ")} from ${found.repo}`);
  return { auth: { ...auth, account_id: who, email: who } };
}
