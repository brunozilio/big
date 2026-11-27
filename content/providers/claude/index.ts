import {
  type Config,
  type Model,
  type Tokens,
  bearer,
  browser,
  EFFORTS,
  cacheMarks,
  capabilities,
  chosen,
  callback,
  form,
  get,
  instant,
  items,
  jsonHeaders,
  jsonRequest,
  lasting,
  lifetime,
  model,
  number,
  option,
  reasoning,
  refreshable,
  remove,
  span,
  standard,
  unoffered,
  window,
  words,
  invalid,
} from "sdk/providers/index.ts";

const HOURS_5 = 18_000;
const WEEK = 604_800;
const WINDOWS: [string, string, number][] = [
  ["five_hour", "", HOURS_5],
  ["seven_day", "", WEEK],
  ["seven_day_opus", "Opus", WEEK],
  ["seven_day_sonnet", "Sonnet", WEEK],
];

export function provider() {
  return {
    id: "claude-subscription",
    name: "Claude Subscription",
    label: "Claude",
    logo: "logo.svg",
    vendor: "anthropic",
    access: "browser",
    rank: 20,
    slot: "0x7072_6f76_6964_6572_0000_0000_0000_0005",
    window: 200_000,
    wire: "messages",
    path: "/messages",
  };
}

const FILTERING_SEARCH = ["fable", "mythos", "opus-5", "opus-4-8", "opus-4-7", "opus-4-6", "sonnet-5", "sonnet-4-6"];

export function webSearch({ name }: { name: string }) {
  const filtering = FILTERING_SEARCH.some((family) => name.includes(family));
  return { type: filtering ? "web_search_20260209" : "web_search_20250305", name: "web_search" };
}

const ADAPTIVE = [/^claude-(opus|sonnet|haiku)-[5-9](-|$)/, /^claude-(opus|sonnet)-4-[6-9](-|$)/];
const adaptive = (name: string) => ADAPTIVE.some((pattern) => pattern.test(name));
const dialed = (name: string) => adaptive(name) || name.startsWith("claude-opus-4-5");

function thinking(body, model: Model | null, effort: string) {
  const caps = chosen(model, body);
  const name = typeof body.model === "string" ? body.model : "";
  const thinks = adaptive(name) || caps?.adaptive_thinking === true;
  if (!thinks && !name.startsWith("claude-opus-4-5")) return body;
  if (unoffered(caps, effort)) invalid(`model ${name} does not support effort ${effort}`);
  body.output_config = { effort };
  if (thinks) body.thinking = { type: "adaptive" };
  return body;
}

export function check({ config }: { config: Config }) {
  refreshable(config);
}

export const base = (_: unknown, ctx) => ctx.config.api;

export function headers({ config }: { config: Config }, ctx) {
  const sent: [string, string][] = [["accept", "text/event-stream"]];
  if (config.auth) {
    sent.push(
      bearer(config.auth.access_token),
      ["anthropic-version", "2023-06-01"],
      ["anthropic-beta", ctx.config.anthropic_beta],
      ["anthropic-dangerous-direct-browser-access", "true"],
      ["anthropic-client-platform", ctx.config.client_platform],
      ["anthropic-client-version", ctx.config.client_version],
      ["x-app", "cli"],
      ["user-agent", ctx.config.user_agent],
    );
  }
  return sent;
}

export function shape({ model, body, effort, intent }, ctx) {
  const existing = Array.isArray(body.system) ? body.system : [];
  body.system = [{ type: "text", text: "You are Claude Code, Anthropic's official CLI for Claude." }, ...existing];
  const ttl = lifetime(intent, { long: ctx.config.cache_long, short: ctx.config.cache_short });
  cacheMarks(body, ttl ? { type: "ephemeral", ttl } : null);
  body.max_tokens = ctx.models.outputLimit({ model, body });
  remove(body, ["thinking", "output_config"]);
  return thinking(body, model, effort);
}

export const catalog = ({ config }: { config: Config }, ctx) => get(config, ctx.config.api, headers({ config }, ctx), "/models?limit=1000");

export function page({ value }, ctx) {
  const models = items(value, "data")
    .map((item) => model(ctx, item))
    .filter((found) => found !== null)
    .map((found) => reasoning(found, true));
  return { models, next: null };
}

export function finish({ models }: { models: Model[] }, ctx) {
  if (models.length > 0) return models;
  return words(ctx.config.fallback_models).map((id) => {
    const fallback = option(ctx, id, id);
    if (dialed(id)) fallback.capabilities = capabilities({ reasoning_levels: EFFORTS });
    return fallback;
  });
}

export function limits({ config }: { config: Config }, ctx) {
  return { url: ctx.config.usage_url, headers: jsonHeaders(headers({ config }, ctx)), body: null };
}

export function windows({ value, language }, ctx) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) invalid("Claude usage is not an object");
  const found = WINDOWS.flatMap(([id, family, length]) => {
    const entry = value[id] ?? {};
    const used = number(entry.utilization);
    if (used === undefined) return [];
    const spanned = span(ctx, length, language);
    const label = family === "" ? spanned : `${spanned} (${family})`;
    return [lasting(ctx, window(id, label, used, instant(ctx, entry.resets_at)), length)];
  });
  return { plan: null, windows: found };
}

function request(ctx, pairs: [string, string][]) {
  const body = Object.fromEntries([...pairs, ["client_id", ctx.config.client_id]]);
  return jsonRequest(ctx.config.token_url, body);
}

export function refresh({ previous }: { previous: Tokens }, ctx) {
  return request(ctx, [
    ["grant_type", "refresh_token"],
    ["refresh_token", previous.refresh_token ?? ""],
  ]);
}

export const tokens = (input, ctx) => standard(ctx, input);

function authorize(ctx, challenge: string, state: string, redirect: string) {
  const query = form(ctx, [
    ["code", "true"],
    ["client_id", ctx.config.client_id],
    ["response_type", "code"],
    ["redirect_uri", redirect],
    ["scope", ctx.config.scopes],
    ["code_challenge", challenge],
    ["code_challenge_method", "S256"],
    ["state", state],
  ]);
  return `${ctx.config.authorize_url}?${query}`;
}

export function signIn(_: unknown, ctx) {
  const auth = browser(ctx, {
    callback: callback(ctx.config.callback_port, ctx.config.callback_path),
    stateBytes: 32,
    authorize: (challenge, state, redirect) => authorize(ctx, challenge, state, redirect),
    exchange: (code, state, verifier, redirect) =>
      request(ctx, [
        ["grant_type", "authorization_code"],
        ["code", code],
        ["code_verifier", verifier],
        ["redirect_uri", redirect],
        ["state", state],
      ]),
    tokens,
  });
  return { auth };
}
