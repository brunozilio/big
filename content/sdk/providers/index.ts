export type Tokens = {
  access_token: string;
  refresh_token?: string;
  account_id?: string;
  expires_at?: string | null;
  client?: unknown;
  email?: string | null;
  subject?: string | null;
};
export type Config = { api_key?: string; base_url?: string; auth?: Tokens | null };
export type Headers = [string, string][];
export type Call = { url: string; headers: Headers; body?: string | null };
export type TokenRequest = { url: string; contentType: string; body: string };
export type Model = {
  id: string;
  label: string;
  provider?: string;
  window?: number | null;
  capabilities?: Record<string, unknown> | null;
};
export type CacheWrite = number | { long?: number; short?: number };
export type Rates = { input: number; output: number; cacheRead: number; cacheWrite: CacheWrite };
export type Tier = Rates & { above: number };
export type Price = Rates & { tiers: Tier[]; laterDiscount?: number };
export type Prices = Record<string, Price>;
export type Basis = "subscription" | "api";
export type Training = "yes" | "no" | "unknown";
export type Retention = number | "none" | "unknown";
export type DataHandling = { training: Training; retention: Retention };
export type Handling = { region?: string; dataHandling?: Partial<DataHandling> };

export const over = (above: number, input: number, output: number, cacheRead: number, cacheWrite: CacheWrite): Tier => ({
  above,
  input,
  output,
  cacheRead,
  cacheWrite,
});

export const usd = (input: number, output: number, cacheRead: number, cacheWrite: CacheWrite, ...tiers: Tier[]): Price => ({
  input,
  output,
  cacheRead,
  cacheWrite,
  tiers,
});

export type Window = { id: string; label: string; used_percent: number; resets_at: string | null; starts_at?: string | null };
export type Reached = { status: number; body: string };

const thrown = (kind: string, message: string) => new Error(JSON.stringify({ kind, message }));

export function unauthorized(message: string): never {
  throw thrown("unauthorized", message);
}

export function invalid(message: string): never {
  throw thrown("invalid", message);
}

export function described(error: unknown): string {
  const message = String((error as Error)?.message ?? error);
  try {
    const parsed = JSON.parse(message);
    if (parsed?.kind === "unauthorized") return `not signed in: ${parsed.message}`;
    if (parsed?.message) return String(parsed.message);
  } catch (_) {}
  return message;
}

export const words = (text: unknown): string[] => (typeof text === "string" ? text.split(/\s+/).filter((word) => word !== "") : []);

export const regional = (template: string, region: string) => template.replaceAll("{region}", region);

export const callback = (port: string, path: string): [number, string] => [Number.parseInt(port, 10) || 0, path];

export const text = (value: unknown): string | undefined => (typeof value === "string" && value.trim() !== "" ? value : undefined);

export const accessToken = (config: Config) => config.auth?.access_token ?? "";

export const account = (config: Config) => config.auth?.account_id ?? "";

export function refreshable(config: Config): Tokens {
  const tokens = config.auth;
  if (!tokens) unauthorized("subscription credential is missing");
  if (!tokens.access_token || !tokens.refresh_token) unauthorized("subscription needs access_token and refresh_token");
  return tokens;
}

export const baseOf = (config: Config, own: string) => (config.base_url ? config.base_url.replace(/\/+$/, "") : own);

export function get(config: Config, own: string, headers: Headers, path: string): Call {
  return { url: `${baseOf(config, own)}${path}`, headers, body: null };
}

export function jsonHeaders(headers: Headers): Headers {
  return [...headers.filter(([name]) => name.toLowerCase() !== "accept"), ["accept", "application/json"]];
}

export const bearer = (token: string): [string, string] => ["authorization", `Bearer ${token}`];

export function form(ctx, pairs: [string, string][]): string {
  return ctx.oauth.form(pairs);
}

export function formRequest(ctx, url: string, pairs: [string, string][]): TokenRequest {
  return { url, contentType: "application/x-www-form-urlencoded", body: form(ctx, pairs) };
}

export function jsonRequest(url: string, body: unknown): TokenRequest {
  return { url, contentType: "application/json", body: JSON.stringify(body) };
}

export function post(ctx, request: TokenRequest): Reached {
  return ctx.http.post({ url: request.url, headers: [["content-type", request.contentType]], body: request.body });
}

export function parsed(body: string): any {
  try {
    return JSON.parse(body);
  } catch (_) {
    return undefined;
  }
}

export function claims(ctx, jwt: string): any {
  const parts = jwt.split(".");
  if (parts.length !== 3) return undefined;
  try {
    return JSON.parse(ctx.encoding.convert({ data: parts[1], from: "base64url", to: "text" }));
  } catch (_) {
    return undefined;
  }
}

export const later = (ctx, at: string, seconds: number): string | null => ctx.clock.later({ at, seconds });

export function standard(ctx, input: { status: number; body: string; previous?: Tokens | null; now: string }): Tokens {
  return ctx.oauth.standard({ status: input.status, body: input.body, previous: input.previous ?? null, now: input.now });
}

export type Browser = {
  callback: [number, string];
  stateBytes: number;
  authorize: (challenge: string, state: string, redirect: string) => string;
  exchange: (code: string, state: string, verifier: string, redirect: string) => TokenRequest;
  tokens: (input: { status: number; body: string; previous: Tokens | null; now: string }, ctx) => Tokens;
};

export function browser(ctx, flow: Browser): Tokens {
  const [port, path] = flow.callback;
  const listened = ctx.callback.listen({ port, path });
  const verifier = ctx.crypto.random(64);
  const state = ctx.crypto.random(flow.stateBytes);
  ctx.browser.open({ url: flow.authorize(ctx.oauth.challenge(verifier), state, listened.redirect), code: null });
  const answer = ctx.callback.next({ listener: listened.listener, state, others: [] }).answer;
  try {
    const code = ctx.callback.code({ answer });
    const { status, body } = post(ctx, flow.exchange(code, state, verifier, listened.redirect));
    const tokens = flow.tokens({ status, body, previous: null, now: ctx.clock.now() }, ctx);
    ctx.callback.signedIn({ answer, account: ctx.oauth.account(body) });
    return tokens;
  } catch (error) {
    ctx.callback.finish({ answer, error: described(error) });
    throw error;
  }
}

export function tryPost(ctx, url: string, headers: Headers, body: string): Reached | undefined {
  try {
    return ctx.http.post({ url, headers, body });
  } catch (_) {
    return undefined;
  }
}

export function items(value: any, key: string): any[] {
  const found = value?.[key];
  return Array.isArray(found) ? found : invalid("model catalog has no model array");
}

export const string = (item: any, key: string): string => (typeof item?.[key] === "string" ? item[key].trim() : "");

export const positive = (value: unknown): number | null => (typeof value === "number" && Number.isInteger(value) && value > 0 ? value : null);

export function option(ctx, id: string, label: string): Model {
  return ctx.catalog.option({ id, label });
}

export function model(ctx, item: unknown, fields?: { id: string; window: string }): Model | null {
  return ctx.catalog.model({ item, id: fields?.id ?? "id", window: fields?.window ?? "max_input_tokens" });
}

export function capabilities(caps: Record<string, unknown>) {
  return {
    reasoning: false,
    adaptive_thinking: false,
    max_output_tokens: null,
    auto_compact_token_limit: null,
    effective_context_window_percent: null,
    reasoning_levels: [],
    endpoints: [],
    ...caps,
  };
}

export function reasoning(model: Model, reasons: boolean): Model {
  if (model.capabilities) model.capabilities.reasoning = reasons;
  return model;
}

export const levels = (item: any): string[] =>
  Array.isArray(item?.supported_reasoning_levels) ? item.supported_reasoning_levels.map((level) => level?.effort).filter((effort) => typeof effort === "string") : [];

export function number(value: unknown): number | undefined {
  if (typeof value === "number") return value;
  if (typeof value === "string" && value.trim() !== "" && !Number.isNaN(Number(value.trim()))) return Number(value.trim());
  return undefined;
}

export function window(id: string, label: string, used: number, resetsAt: string | null | undefined): Window {
  return { id, label, used_percent: Math.min(100, Math.max(0, used)), resets_at: resetsAt ?? null, starts_at: null };
}

export const lasting = (ctx, shaped: Window, seconds: number): Window => ctx.limits.lasting({ window: shaped, seconds });

export const monthly = (ctx, shaped: Window): Window => ctx.limits.monthly(shaped);

export const span = (ctx, seconds: number, language: string): string => ctx.limits.span({ seconds, language });

export const instant = (ctx, value: unknown): string | null => (typeof value === "string" ? ctx.limits.instant(value) : null);

export const epoch = (ctx, seconds: number): string | null => ctx.limits.epoch({ seconds });

export const after = (ctx, seconds: number): string | null => ctx.limits.after({ seconds });

export const titled = (ctx, value: unknown): string | null => (typeof value === "string" ? ctx.limits.titled(value) : null);

export function pick<T>(language: string, english: T, spanish: T, portuguese: T): T {
  if (language === "es") return spanish;
  if (language === "pt-BR" || language === "pt") return portuguese;
  return english;
}

export function remove(body: Record<string, unknown>, keys: string[]) {
  for (const key of keys) delete body[key];
  return body;
}

export type Intent = { cache?: "long" | "short" | "none"; priority?: "now" | "later"; key?: string } | null | undefined;

export function lifetime<T>(intent: Intent, lifetimes: { long: T; short: T }): T | null {
  const cache = intent?.cache ?? "short";
  if (cache === "none") return null;
  return cache === "long" ? lifetimes.long : lifetimes.short;
}

export function cacheMarks(body, mark: Record<string, unknown> | null) {
  if (mark === null) return body;
  body.cache_control = { ...mark };
  for (const list of [body.system, body.tools]) {
    const last = Array.isArray(list) ? list[list.length - 1] : undefined;
    if (last && typeof last === "object") last.cache_control = { ...mark };
  }
  return body;
}

export function cacheKey(body, intent: Intent) {
  if (typeof intent?.key === "string") body.prompt_cache_key = intent.key;
  return body;
}

export function jsonLines(text: string): any[] {
  return String(text ?? "")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "")
    .flatMap((line) => {
      const found = parsed(line);
      return found === undefined ? [] : [found];
    });
}

function event(data: Record<string, unknown>): string {
  return `event: ${data.type}\ndata: ${JSON.stringify(data)}\n\n`;
}

function blockEvents(block, index: number): string[] {
  const kind = block?.type;
  if (kind === "text") {
    return [
      event({ type: "content_block_start", index, content_block: { type: "text", text: "" } }),
      event({ type: "content_block_delta", index, delta: { type: "text_delta", text: block.text ?? "" } }),
    ];
  }
  if (kind === "thinking") {
    return [
      event({ type: "content_block_start", index, content_block: { type: "thinking", thinking: "" } }),
      event({ type: "content_block_delta", index, delta: { type: "thinking_delta", thinking: block.thinking ?? "" } }),
      event({ type: "content_block_delta", index, delta: { type: "signature_delta", signature: block.signature ?? "" } }),
    ];
  }
  if (kind === "tool_use") {
    return [
      event({ type: "content_block_start", index, content_block: { type: "tool_use", id: block.id, name: block.name, input: {} } }),
      event({ type: "content_block_delta", index, delta: { type: "input_json_delta", partial_json: JSON.stringify(block.input ?? {}) } }),
    ];
  }
  return [event({ type: "content_block_start", index, content_block: block })];
}

export function messagesStream(message): string {
  const blocks = Array.isArray(message?.content) ? message.content : [];
  const usage = message?.usage ?? {};
  const parts = [event({ type: "message_start", message: { ...message, content: [], usage } })];
  blocks.forEach((block, index) => parts.push(...blockEvents(block, index), event({ type: "content_block_stop", index })));
  parts.push(event({ type: "message_delta", delta: { stop_reason: message?.stop_reason ?? null }, usage }));
  parts.push(event({ type: "message_stop" }));
  return parts.join("");
}

export const EFFORTS = ["low", "medium", "high", "xhigh", "max"];

export function chosen(model: Model | null | undefined, body): Record<string, any> | undefined {
  const id = typeof model?.id === "string" ? model.id.split(":").pop() : undefined;
  return id !== undefined && id === body?.model && model?.capabilities ? model.capabilities : undefined;
}

export function unoffered(caps: Record<string, any> | undefined, effort: string): boolean {
  const levels = Array.isArray(caps?.reasoning_levels) ? caps.reasoning_levels : [];
  return levels.length > 0 && !levels.includes(effort);
}

export function nearest(levels: string[], effort: string): string {
  const wanted = EFFORTS.indexOf(effort);
  const asked = wanted < 0 ? 1 : wanted;
  const below = [...EFFORTS.slice(0, asked + 1)].reverse().find((level) => levels.includes(level));
  return below ?? EFFORTS.find((level) => levels.includes(level)) ?? "medium";
}
