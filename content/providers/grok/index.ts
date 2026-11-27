import {
  type Config,
  type Model,
  type Reached,
  type Tokens,
  accessToken,
  bearer,
  cacheKey,
  capabilities,
  formRequest,
  get,
  items,
  option,
  parsed,
  positive,
  refreshable,
  standard,
  string,
  text,
  tryPost,
  unauthorized,
} from "sdk/providers/index.ts";

const GRANT = "urn:ietf:params:oauth:grant-type:device_code";
const SKIPPED = ["imagine", "image", "embedding", "tts", "video"];
const MAX_INTERVAL = 60;
const FORM: [string, string][] = [
  ["content-type", "application/x-www-form-urlencoded"],
  ["accept", "application/json"],
];

export function provider() {
  return {
    id: "grok-subscription",
    name: "Grok Subscription",
    label: "Grok",
    logo: "logo.svg",
    vendor: "xai",
    access: "code",
    rank: 60,
    slot: "0x7072_6f76_6964_6572_0000_0000_0000_0008",
    window: 500_000,
    wire: "responses",
    path: "/responses",
  };
}

export const webSearch = () => ({ type: "web_search" });

export function check({ config }: { config: Config }) {
  refreshable(config);
}

export const base = (_: unknown, ctx) => ctx.config.api;

export function headers({ config }: { config: Config }) {
  const sent: [string, string][] = [["accept", "text/event-stream"]];
  if (config.auth) sent.push(bearer(accessToken(config)));
  return sent;
}

export const retriesAfterRefresh = ({ status }: { status: number }) => status === 401;

export const shape = ({ model, body, effort, intent }, ctx) => ctx.models.effort({ vendor: "xai", body: cacheKey(body, intent), model, effort, always: false });

export const catalog = ({ config }: { config: Config }, ctx) => get(config, ctx.config.api, headers({ config }), "/models");

function windowOf(item): number | null {
  const all = Array.isArray(item?.context_windows) ? item.context_windows.filter((n) => typeof n === "number") : [];
  const largest = all.length > 0 ? Math.max(...all) : null;
  return positive(largest ?? item?.context_window ?? item?.context_length);
}

function modelOf(ctx, item): Model | null {
  const id = string(item, "id");
  const lower = id.toLowerCase();
  if (!lower.startsWith("grok") || SKIPPED.some((part) => lower.includes(part))) return null;
  const model = option(ctx, id, `${id} (Grok)`);
  model.window = windowOf(item);
  const reasons = typeof item?.supports_reasoning_effort === "boolean" ? item.supports_reasoning_effort : ctx.models.reasons({ vendor: "xai", id });
  model.capabilities = capabilities({ reasoning: reasons });
  return model;
}

export function page({ value }, ctx) {
  return { models: items(value, "data").map((item) => modelOf(ctx, item)).filter((model) => model !== null), next: null };
}

export function finish({ models }: { models: Model[] }) {
  return [...models].sort((a, b) => (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
}

export function refresh({ previous }: { previous: Tokens }, ctx) {
  const pairs: [string, string][] = [
    ["grant_type", "refresh_token"],
    ["client_id", ctx.config.client_id],
    ["refresh_token", previous.refresh_token ?? ""],
  ];
  return formRequest(ctx, ctx.config.token_url, pairs);
}

export const tokens = (input, ctx) => standard(ctx, input);

function device(ctx, reached: Reached) {
  const failed = () => unauthorized(`xAI refused the device sign-in (HTTP ${reached.status})`);
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
  if (value.error === "authorization_pending") return "pending";
  if (value.error === "slow_down") return "slow";
  if (typeof value.error === "string") unauthorized(text(value.error_description) ?? value.error);
  return unauthorized(`xAI sign-in failed (HTTP ${reached.status})`);
}

function wait(ctx, code): Reached {
  const body = ctx.oauth.form([
    ["grant_type", GRANT],
    ["client_id", ctx.config.client_id],
    ["device_code", code.device_code],
  ]);
  let pause = code.interval;
  let waited = 0;
  for (;;) {
    if (waited >= code.expires_in) unauthorized("the xAI code expired before it was approved");
    ctx.clock.sleep(pause);
    waited += pause;
    const reached = tryPost(ctx, ctx.config.token_url, FORM, body);
    if (reached === undefined) continue;
    const state = polled(reached);
    if (state === "granted") return reached;
    if (state === "slow") pause = Math.min(pause + 5, MAX_INTERVAL);
  }
}

export function signIn(_: unknown, ctx) {
  const body = ctx.oauth.form([
    ["client_id", ctx.config.client_id],
    ["scope", ctx.config.scopes],
  ]);
  const code = device(ctx, ctx.http.post({ url: ctx.config.device_code_url, headers: FORM, body }));
  ctx.browser.open({ url: code.open, code: code.user_code });
  const { status, body: granted } = wait(ctx, code);
  return { auth: tokens({ status, body: granted, previous: null, now: ctx.clock.now() }, ctx) };
}
