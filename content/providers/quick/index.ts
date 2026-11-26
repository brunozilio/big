import { type Config, type Tokens, epoch, invalid, option, parsed, pick, unauthorized, window, words } from "sdk/providers/index.ts";
import { frame as read, modeOf, sent } from "./frames.ts";
import { clientOf, refresh as refreshWith, signIn as signInWith, tokens as tokensOf } from "./sign_in.ts";

const ID = "quick-subscription";

export function provider() {
  return {
    id: ID,
    name: "Amazon Quick Subscription",
    label: "Amazon Quick",
    logo: "logo.svg",
    vendor: "",
    access: "browser",
    rank: 90,
    slot: "0x7072_6f76_6964_6572_0000_0000_0000_000a",
    window: 200_000,
    wire: "socket",
    framing: "aws-eventstream",
    path: "",
    servesAll: true,
  };
}

const modes = (ctx) => words(ctx.config.modes);

function signedIn(config: Config) {
  const client = clientOf(config.auth);
  if (!config.auth?.access_token || !client) unauthorized("sign in to Amazon Quick again");
  return { token: config.auth.access_token, client };
}

export function check({ config }: { config: Config }) {
  signedIn(config);
}

export function base({ config }: { config: Config }, ctx) {
  const { token, client } = signedIn(config);
  const host = client.tenant_url.replace(/^https:\/\//, "").replace(/\/.*$/, "");
  const query = [
    ["surfaceType", ctx.config.surface_type],
    ["token", token],
    ["workingSessionId", ctx.crypto.uuid()],
  ]
    .map(([name, value]) => `${name}=${encodeURIComponent(value)}`)
    .join("&");
  return `wss://${host}${ctx.config.chat_path}?${query}`;
}

export const headers = () => [];

export const retriesAfterRefresh = ({ status }: { status: number }) => status === 401 || status === 403;

export const shape = ({ model, body }, ctx) => sent(ctx, body, modeOf(model?.id ?? body?.model, modes(ctx), ctx.config.default_mode));

export const frame = ({ state, frame: received }, ctx) => read(ctx, state, received);

function invoked(config: Config, ctx, path: string) {
  const { token, client } = signedIn(config);
  const body = { eventType: "invokeAgent", eventBody: { invokeAgent: { path, queryParams: { _method: "GET" }, body: {} } } };
  return {
    url: `${client.tenant_url.replace(/\/+$/, "")}${ctx.config.api_path}`,
    headers: [
      ["authorization", `Bearer ${token}`],
      ["content-type", "application/json"],
      ["accept", "application/json"],
    ],
    body: JSON.stringify(body),
  };
}

export function answered(value) {
  let found = value?.eventResponse?.invokeAgentResponse ?? value;
  if (typeof found === "string") found = parsed(found);
  if (found?.type !== "api_response") return found;
  if (typeof found.status === "number" && found.status >= 400) invalid(`Amazon Quick answered ${found.status}`);
  return typeof found.body === "string" ? parsed(found.body) : found.body;
}

export const catalog = ({ config }: { config: Config }, ctx) => invoked(config, ctx, ctx.config.usage_path);

const LABELS: Record<string, [string, string, string]> = {
  fast: ["Fast", "Rápido", "Rápido"],
  smart: ["Smart", "Inteligente", "Inteligente"],
  advanced: ["Advanced", "Avanzado", "Avançado"],
};

export function page({ value }, ctx) {
  answered(value);
  return { models: modes(ctx).map((mode) => option(ctx, mode, `Amazon Quick · ${LABELS[mode]?.[0] ?? mode}`)), next: null };
}

export function limits({ config }: { config: Config }, ctx) {
  if (!clientOf(config.auth)) return { url: ctx.config.api_path, headers: [], body: "" };
  return invoked(config, ctx, ctx.config.usage_path);
}

function meter(ctx, id: string, label: string, entry) {
  const used = typeof entry?.used === "number" ? entry.used : undefined;
  const limit = typeof entry?.limit === "number" ? entry.limit : undefined;
  if (used === undefined || limit === undefined || limit <= 0) return undefined;
  const resets = typeof entry.resets_at === "number" ? epoch(ctx, entry.resets_at) : null;
  return window(id, `${label} · ${used}/${limit}`, (used / limit) * 100, resets);
}

export function windows({ value, language }, ctx) {
  const usage = answered(value) ?? invalid("Amazon Quick sent no usage");
  const session = meter(ctx, "session", pick(language, "Session", "Sesión", "Sessão"), usage.session);
  const monthly = meter(ctx, "monthly", pick(language, "Month", "Mes", "Mês"), usage.monthly);
  const plan = typeof usage.subscription?.subscription_type === "string" ? usage.subscription.subscription_type : null;
  return { plan, windows: [session, monthly].filter((found) => found !== undefined) };
}

export function refresh({ previous }: { previous: Tokens }, ctx) {
  const client = clientOf(previous);
  return client ? refreshWith(ctx, client, previous) : null;
}

export function tokens(input, ctx) {
  const client = clientOf(input.previous) ?? unauthorized("sign in to Amazon Quick again");
  return tokensOf(ctx, client, input.previous?.account_id ?? "", input);
}

export const signIn = (_: unknown, ctx) => ({ auth: signInWith(ctx) });
