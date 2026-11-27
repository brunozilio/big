import {
  type Config,
  type Tokens,
  after,
  bearer,
  browser,
  cacheKey,
  chosen,
  callback,
  claims,
  epoch,
  form,
  formRequest,
  get,
  invalid,
  items,
  jsonHeaders,
  lasting,
  levels,
  model,
  number,
  parsed,
  reasoning,
  unoffered,
  refreshable,
  remove,
  span,
  standard,
  text,
  titled,
  unauthorized,
  window,
} from "sdk/providers/index.ts";
import { OPENAI } from "../prices/openai.ts";

const USAGE = "/wham/usage";
const WINDOWS: [string, string][] = [
  ["primary_window", "primary"],
  ["secondary_window", "secondary"],
];

export function provider() {
  return {
    id: "openai-subscription",
    name: "ChatGPT Subscription",
    label: "ChatGPT",
    logo: "logo.svg",
    access: "browser",
    rank: 40,
    slot: "0x7072_6f76_6964_6572_0000_0000_0000_0004",
    window: 400_000,
    wire: "responses",
    path: "/codex/responses",
    prices: OPENAI,
    searchPrice: 0.01,
    owns: ["gpt-", "o1", "o3", "o4"],
    makers: [["gpt", "openai.svg"], ["codex", "openai.svg"], ["o3", "openai.svg"], ["o4", "openai.svg"]],
    smallModel: "gpt-6-luna",
  };
}

export const webSearch = () => ({ type: "web_search" });

export const toolsInConversation = ({ name }: { name: string }, ctx) => ctx.models.toolsInConversation({ name });

export function check({ config }: { config: Config }) {
  if (!refreshable(config).account_id) unauthorized("ChatGPT account_id is missing");
}

export const base = (_: unknown, ctx) => ctx.config.api;

export function headers({ config }: { config: Config }, ctx) {
  const sent: [string, string][] = [["accept", "text/event-stream"]];
  if (config.auth) {
    sent.push(
      bearer(config.auth.access_token),
      ["chatgpt-account-id", config.auth.account_id ?? ""],
      ["openai-beta", "responses=experimental"],
      ["originator", ctx.config.originator],
      ["version", ctx.config.version],
      ["user-agent", ctx.config.user_agent],
    );
  }
  return sent;
}

export function turnHeaders({ body }) {
  return typeof body?.prompt_cache_key === "string" ? [["session_id", body.prompt_cache_key]] : [];
}

export const retriesAfterRefresh = ({ status }: { status: number }) => status === 401;

const REASONING = ["o1", "o3", "o4", "gpt-5", "gpt-6"];
const LEVELS = ["low", "medium", "high", "xhigh"];

const owned = (slug: string) => slug.startsWith("gpt-") || ["o1", "o3", "o4"].some((prefix) => slug.startsWith(prefix));
const reasons = (slug: string) => owned(slug) && REASONING.some((prefix) => slug.toLowerCase().startsWith(prefix));

export function shape({ model, body, effort, intent }, ctx) {
  cacheKey(body, intent);
  body.instructions = `${ctx.config.instructions}\n\n${typeof body.instructions === "string" ? body.instructions : ""}`;
  body.text = { verbosity: "medium" };
  remove(body, ["parallel_tool_calls", "tool_choice"]);
  if (Array.isArray(body.tools)) for (const tool of body.tools) if (tool && typeof tool === "object") delete tool.strict;
  const asked = effort === "max" ? "xhigh" : effort;
  if (unoffered(chosen(model, body), asked)) invalid(`model ${body.model} does not support reasoning effort ${asked}`);
  body.reasoning = { effort: asked, summary: "auto" };
  return body;
}

export const catalog = ({ config }: { config: Config }, ctx) => get(config, ctx.config.api, headers({ config }, ctx), `/codex/models?client_version=${ctx.config.version}`);

export function page({ value }, ctx) {
  const models = items(value, "models").flatMap((item) => {
    if (item?.visibility === "hide") return [];
    const found = model(ctx, item, { id: "slug", window: "context_window" });
    if (found === null) return [];
    if (levels(item).length === 0 && reasons(found.id.split(":").pop() ?? "") && found.capabilities) found.capabilities.reasoning_levels = LEVELS;
    return [reasoning(found, levels(item).length > 0)];
  });
  return { models, next: null };
}

export function limits({ config }: { config: Config }, ctx) {
  const call = get(config, ctx.config.api, headers({ config }, ctx), USAGE);
  return { ...call, headers: jsonHeaders(call.headers) };
}

function limit(ctx, id: string, entry, language: string) {
  const used = number(entry?.used_percent);
  if (used === undefined) return [];
  const seconds = Number.isInteger(entry?.limit_window_seconds) && entry.limit_window_seconds > 0 ? entry.limit_window_seconds : null;
  const label = seconds === null ? id : span(ctx, seconds, language);
  const resets = (Number.isInteger(entry?.reset_at) ? epoch(ctx, entry.reset_at) : null) ?? (Number.isInteger(entry?.reset_after_seconds) ? after(ctx, entry.reset_after_seconds) : null);
  const shaped = window(id, label, used, resets);
  return [seconds === null ? shaped : lasting(ctx, shaped, seconds)];
}

export function windows({ value, language }, ctx) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) invalid("ChatGPT usage is not an object");
  const found = WINDOWS.flatMap(([key, id]) => limit(ctx, id, value.rate_limit?.[key], language));
  return { plan: titled(ctx, value.plan_type), windows: found };
}

function request(ctx, pairs: [string, string][]) {
  return formRequest(ctx, ctx.config.token_url, [...pairs, ["client_id", ctx.config.client_id]]);
}

export function refresh({ previous }: { previous: Tokens }, ctx) {
  return request(ctx, [
    ["grant_type", "refresh_token"],
    ["refresh_token", previous.refresh_token ?? ""],
  ]);
}

function accountId(ctx, jwt: string): string | undefined {
  const found = claims(ctx, jwt);
  return text(found?.[ctx.config.account_claim]?.chatgpt_account_id) ?? text(found?.chatgpt_account_id) ?? text(found?.organizations?.[0]?.id);
}

export function tokens(input, ctx) {
  const fresh = standard(ctx, input);
  const value = parsed(input.body) ?? {};
  const found =
    accountId(ctx, typeof value.id_token === "string" ? value.id_token : "") ??
    accountId(ctx, typeof value.access_token === "string" ? value.access_token : "") ??
    text(input.previous?.account_id);
  if (found === undefined) unauthorized("ChatGPT access token has no account id");
  return { ...fresh, account_id: found };
}

function authorize(ctx, challenge: string, state: string, redirect: string) {
  const query = form(ctx, [
    ["response_type", "code"],
    ["client_id", ctx.config.client_id],
    ["redirect_uri", redirect],
    ["scope", ctx.config.scopes],
    ["code_challenge", challenge],
    ["code_challenge_method", "S256"],
    ["state", state],
    ["id_token_add_organizations", "true"],
    ["codex_cli_simplified_flow", "true"],
    ["originator", ctx.config.originator],
  ]);
  return `${ctx.config.authorize_url}?${query}`;
}

export function signIn(_: unknown, ctx) {
  const auth = browser(ctx, {
    callback: callback(ctx.config.callback_port, ctx.config.callback_path),
    stateBytes: 16,
    authorize: (challenge, state, redirect) => authorize(ctx, challenge, state, redirect),
    exchange: (code, _state, verifier, redirect) =>
      request(ctx, [
        ["grant_type", "authorization_code"],
        ["code", code],
        ["code_verifier", verifier],
        ["redirect_uri", redirect],
      ]),
    tokens,
  });
  return { auth };
}
