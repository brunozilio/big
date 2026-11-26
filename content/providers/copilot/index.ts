import {
  type Config,
  type Model,
  type Reached,
  bearer,
  capabilities,
  get,
  instant,
  invalid,
  items,
  monthly,
  number,
  option,
  parsed,
  pick,
  positive,
  text,
  titled,
  tryPost,
  unauthorized,
  window,
} from "sdk/providers/index.ts";

const GRANT = "urn:ietf:params:oauth:grant-type:device_code";
const MAX_INTERVAL = 60;
const BUDGET_FLOOR = 1024;
const ORDER = ["low", "medium", "high", "xhigh", "max"];
const FORM: [string, string][] = [
  ["content-type", "application/x-www-form-urlencoded"],
  ["accept", "application/json"],
];
const QUOTAS: [string, [string, string, string]][] = [
  ["premium_interactions", ["Premium requests", "Solicitudes premium", "Requisições premium"]],
  ["chat", ["Chat", "Chat", "Chat"]],
  ["completions", ["Completions", "Autocompletado", "Autocompletar"]],
];
const PATHS: Record<string, string> = { messages: "/v1/messages", responses: "/responses", chat: "/chat/completions" };

export function provider() {
  return {
    id: "copilot-subscription",
    name: "GitHub Copilot Subscription",
    label: "GitHub Copilot",
    logo: "logo.svg",
    vendor: "github",
    access: "code",
    rank: 50,
    slot: "0x7072_6f76_6964_6572_0000_0000_0000_0006",
    window: 131_072,
    wire: "chat",
    path: "/chat/completions",
  };
}

function wireOf(model: Model | null | undefined): string {
  const endpoints = (model?.capabilities?.endpoints as string[] | undefined) ?? [];
  if (endpoints.includes("/v1/messages")) return "messages";
  if (endpoints.includes("/responses")) return "responses";
  return "chat";
}

export const wire = ({ model }) => wireOf(model);

export const path = ({ model }) => PATHS[wireOf(model)] ?? "/";

export const bare = ({ raw }: { raw: string }) => ({ auth: { access_token: raw.trim() } });

export function check({ config }: { config: Config }) {
  const token = config.auth?.access_token ?? "";
  if (token === "" || /\s/.test(token)) unauthorized("GitHub token is empty or contains whitespace");
  if (token.startsWith("ghp_")) {
    unauthorized(
      "classic personal access tokens (ghp_) do not work with Copilot; sign in, or use a fine-grained token with the Copilot Requests permission",
    );
  }
}

export const base = (_: unknown, ctx) => ctx.config.api;

const userAgent = (ctx) => `copilot/${ctx.config.version} (${ctx.platform.os}) term/unknown`;

function signed(ctx, token: string): [string, string][] {
  return [
    bearer(token),
    ["copilot-integration-id", ctx.config.integration],
    ["editor-version", `copilot/${ctx.config.version}`],
    ["user-agent", userAgent(ctx)],
    ["openai-intent", "conversation-agent"],
    ["x-github-api-version", ctx.config.api_version],
  ];
}

export function headers({ config }: { config: Config }, ctx) {
  const sent: [string, string][] = [["accept", "text/event-stream"]];
  if (config.auth) sent.push(...signed(ctx, config.auth.access_token));
  return sent;
}

function startedByUser(body): boolean {
  const list = Array.isArray(body?.messages) ? body.messages : Array.isArray(body?.input) ? body.input : [];
  let user = false;
  for (const item of [...list].reverse()) {
    const role = item?.role ?? "";
    const kind = item?.type ?? "";
    if (role === "assistant" || kind === "function_call" || kind === "reasoning") break;
    const toolResult = role === "tool" || kind === "function_call_output" || (Array.isArray(item?.content) && item.content.some((block) => block?.type === "tool_result"));
    if (toolResult) return false;
    user ||= role === "user";
  }
  return user;
}

function hasImage(value): boolean {
  if (Array.isArray(value)) return value.some(hasImage);
  if (value === null || typeof value !== "object") return false;
  return ["image", "image_url", "input_image"].includes(value.type) || Object.values(value).some(hasImage);
}

export function turnHeaders({ model, body }) {
  const byUser = startedByUser(body);
  const sent: [string, string][] = [
    ["x-initiator", byUser ? "user" : "agent"],
    ["x-interaction-type", byUser ? "conversation-user" : "conversation-agent"],
  ];
  if (hasImage(body)) sent.push(["copilot-vision-request", "true"]);
  if (wireOf(model) === "messages") sent.push(["anthropic-version", "2023-06-01"]);
  return sent;
}

function level(levels: string[], effort: string): string {
  const found = ORDER.indexOf(effort);
  const asked = found < 0 ? 1 : found;
  if (levels.length === 0) return ORDER[Math.min(asked, 2)];
  return [...ORDER.slice(0, asked + 1)].reverse().find((each) => levels.includes(each)) ?? levels[0];
}

function budget(effort: string): number {
  if (effort === "low") return BUDGET_FLOOR;
  if (effort === "medium") return BUDGET_FLOOR * 2;
  if (effort === "xhigh" || effort === "max") return 32_000;
  return BUDGET_FLOOR * 4;
}

function messagesShape(body, caps, effort: string, limit: (asked: unknown, fallback: number) => number) {
  const max = limit(body.max_tokens, 8192);
  body.max_tokens = max;
  delete body.output_config;
  delete body.thinking;
  const levels: string[] = caps.reasoning_levels ?? [];
  if (caps.adaptive_thinking || levels.length > 0) {
    body.thinking = { type: "adaptive", display: "summarized" };
    body.output_config = { effort: levels.length === 0 ? effort : level(levels, effort) };
  } else if (caps.reasoning) {
    const most = Math.max(max, BUDGET_FLOOR + 1);
    body.max_tokens = most;
    body.thinking = { type: "enabled", budget_tokens: Math.min(Math.max(budget(effort), BUDGET_FLOOR), most - 1) };
  }
}

export function shape({ model, body, effort }) {
  const caps = model?.capabilities ?? {};
  const limit = (asked: unknown, fallback: number) => {
    const most = typeof caps.max_output_tokens === "number" ? caps.max_output_tokens : fallback;
    return typeof asked === "number" ? Math.min(asked, most) : most;
  };
  const wire = wireOf(model);
  if (wire === "chat") body.max_tokens = limit(body.max_tokens, 16_384);
  if (wire === "responses") {
    if (caps.reasoning || (caps.reasoning_levels ?? []).length > 0) body.reasoning = { effort: level(caps.reasoning_levels ?? [], effort), summary: "auto" };
    else {
      delete body.reasoning;
      delete body.include;
    }
  }
  if (wire === "messages") messagesShape(body, caps, effort, limit);
  return body;
}

export const catalog = ({ config }: { config: Config }, ctx) => get(config, ctx.config.api, headers({ config }, ctx), "/models");

function modelOf(ctx, item): Model | null {
  const id = typeof item?.id === "string" ? item.id.trim() : "";
  if (id === "") return null;
  const picked = item.model_picker_enabled !== false || item.policy?.state === "enabled";
  const kind = item.capabilities?.type;
  const chats = typeof kind !== "string" || kind === "chat";
  if (item.custom_model === true || !picked || !chats) return null;
  const limits = item.capabilities?.limits ?? {};
  const supports = item.capabilities?.supports ?? {};
  const found = option(ctx, id, text(item.name)?.trim() ?? id);
  found.window = positive(limits.max_context_window_tokens);
  const levels = Array.isArray(supports.reasoning_effort) ? supports.reasoning_effort.filter((each) => typeof each === "string") : [];
  found.capabilities = capabilities({
    reasoning: levels.length > 0 || supports.reasoning_effort === true || supports.thinking === true || (positive(supports.max_thinking_budget) ?? 0) > 0,
    adaptive_thinking: supports.adaptive_thinking === true,
    max_output_tokens: positive(limits.max_output_tokens),
    reasoning_levels: levels,
    endpoints: Array.isArray(item.supported_endpoints) ? item.supported_endpoints.filter((each) => typeof each === "string") : [],
  });
  return found;
}

export function page({ value }, ctx) {
  return { models: items(value, "data").map((item) => modelOf(ctx, item)).filter((found) => found !== null), next: null };
}

function userHeaders(ctx, token: string): [string, string][] {
  return [bearer(token), ["accept", "application/vnd.github+json"], ["x-github-api-version", "2022-11-28"], ["user-agent", userAgent(ctx)]];
}

export const limits = ({ config }: { config: Config }, ctx) => ({ url: ctx.config.user_url, headers: userHeaders(ctx, config.auth?.access_token ?? ""), body: null });

export function windows({ value, language }, ctx) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) invalid("the Copilot user is not an object");
  const resets = instant(ctx, value.quota_reset_date_utc) ?? instant(ctx, value.quota_reset_date);
  const found = QUOTAS.flatMap(([id, [en, es, pt]]) => {
    const entry = value.quota_snapshots?.[id];
    const remaining = number(entry?.percent_remaining);
    if (entry?.unlimited === true || remaining === undefined) return [];
    return [monthly(ctx, window(id, pick(language, en, es, pt), 100 - remaining, resets))];
  });
  return { plan: titled(ctx, value.copilot_plan), windows: found };
}

function device(reached: Reached) {
  const failed = () => unauthorized(`GitHub refused the device sign-in (HTTP ${reached.status})`);
  if (reached.status !== 200) failed();
  const value = parsed(reached.body) ?? failed();
  return {
    device_code: text(value.device_code) ?? failed(),
    user_code: text(value.user_code) ?? failed(),
    verification_uri: text(value.verification_uri) ?? failed(),
    interval: Math.max(1, Number.isInteger(value.interval) ? value.interval : 5),
    expires_in: Number.isInteger(value.expires_in) ? value.expires_in : 900,
  };
}

function polled(reached: Reached): { token: string } | "pending" | { slow: number | undefined } {
  const value = parsed(reached.body) ?? {};
  const token = text(value.access_token);
  if (token !== undefined) return { token };
  if (value.error === "authorization_pending") return "pending";
  if (value.error === "slow_down") return { slow: Number.isInteger(value.interval) ? value.interval : undefined };
  if (typeof value.error === "string") unauthorized(text(value.error_description) ?? value.error);
  return unauthorized(`GitHub sign-in failed (HTTP ${reached.status})`);
}

function wait(ctx, code): string {
  const body = ctx.oauth.form([
    ["client_id", ctx.config.client_id],
    ["device_code", code.device_code],
    ["grant_type", GRANT],
  ]);
  let pause = Math.ceil((code.interval * 6) / 5);
  let slowed = 0;
  let waited = 0;
  for (;;) {
    if (waited >= code.expires_in) unauthorized("the GitHub code expired before it was approved");
    ctx.clock.sleep(pause);
    waited += pause;
    const reached = tryPost(ctx, ctx.config.token_url, FORM, body);
    if (reached === undefined) continue;
    const state = polled(reached);
    if (state === "pending") continue;
    if ("token" in state) return state.token;
    slowed += 1;
    pause = Math.min((state.slow ?? code.interval) + 5 * slowed, MAX_INTERVAL);
  }
}

function apiBase(ctx, reached: Reached): string {
  if ([401, 403, 404].includes(reached.status)) unauthorized("this GitHub account has no Copilot subscription");
  if (reached.status !== 200) unauthorized(`GitHub returned HTTP ${reached.status} for the Copilot user`);
  const value = parsed(reached.body) ?? unauthorized("invalid Copilot user response");
  const api = typeof value.endpoints?.api === "string" ? value.endpoints.api.replace(/\/+$/, "") : "";
  return api.startsWith("https://") ? api : ctx.config.api;
}

export function signIn(_: unknown, ctx) {
  const body = ctx.oauth.form([
    ["client_id", ctx.config.client_id],
    ["scope", ctx.config.scopes],
  ]);
  const code = device(ctx.http.post({ url: ctx.config.device_code_url, headers: FORM, body }));
  ctx.browser.open({ url: code.verification_uri, code: code.user_code });
  const token = wait(ctx, code);
  const base_url = apiBase(ctx, ctx.http.get({ url: ctx.config.user_url, headers: userHeaders(ctx, token) }));
  return { auth: { access_token: token }, base_url };
}
