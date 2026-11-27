import { type Config, type Model, type Tokens, browser, callback, capabilities, form, formRequest, option, parsed, positive, refreshable, standard, unauthorized, words } from "sdk/providers/index.ts";
import { VERTEX } from "../prices/vertex.ts";

const FREE_TIER = "free-tier";
const ONBOARD_ATTEMPTS = 10;
const ONBOARD_WAIT = 5;

export function provider() {
  return {
    id: "antigravity-subscription",
    name: "Antigravity Subscription",
    label: "Antigravity",
    logo: "logo.svg",
    access: "browser",
    rank: 80,
    slot: "0x7072_6f76_6964_6572_0000_0000_0000_0009",
    window: 200_000,
    wire: "cloud_code",
    path: "/v1internal:streamGenerateContent?alt=sse",
    servesAll: true,
    prices: VERTEX,
    makers: [["gemini", "google.svg"]],
  };
}

function userAgent(ctx): string {
  const os = ctx.platform.os === "macos" ? "darwin" : ctx.platform.os;
  const arch = ctx.platform.arch === "aarch64" ? "arm64" : ctx.platform.arch === "x86_64" ? "amd64" : ctx.platform.arch;
  return `antigravity/hub/${ctx.config.version} (aidev_client; os_type=${os}; arch=${arch})`;
}

function signed(ctx, accessToken: string): [string, string][] {
  return [
    ["authorization", `Bearer ${accessToken}`],
    ["content-type", "application/json"],
    ["accept", "text/event-stream"],
    ["user-agent", userAgent(ctx)],
  ];
}

export function check({ config }: { config: Config }) {
  if (!refreshable(config).account_id) unauthorized("the Antigravity project is missing; sign in again");
}

export const base = (_: unknown, ctx) => ctx.config.api;

export const headers = ({ config }: { config: Config }, ctx) => signed(ctx, config.auth?.access_token ?? "");

export function bind({ config, body }: { config: Config; body: Record<string, unknown> }) {
  body.project = config.auth?.account_id ?? "";
  return body;
}

const TIERS = ["minimal", "low", "medium", "high"];
const CLAUDE_ROOM = 8192;

function family(name: string): string {
  if (name.includes("claude")) return "claude";
  if (name.startsWith("gemini-2")) return "budget";
  if (name.startsWith("gemini-")) return "levels";
  return "other";
}

const suffixOf = (name: string) => TIERS.find((tier) => name.endsWith(`-${tier}`));

function budget(name: string, level: string): number {
  const [low, medium, high] = name.startsWith("gemini-2.5-flash")
    ? [6144, 12_288, 24_576]
    : name.includes("claude") || name.startsWith("gemini-2.5-pro")
      ? [8192, 16_384, 32_768]
      : [4096, 8192, 16_384];
  if (level === "minimal" || level === "low") return low;
  return level === "medium" ? medium : high;
}

function thinking(name: string, level: string, generation: { maxOutputTokens: number }) {
  const tiered = suffixOf(name) !== undefined;
  switch (family(name)) {
    case "claude": {
      if (!name.includes("thinking") && !tiered) return undefined;
      const room = budget(name, level);
      if (generation.maxOutputTokens <= room) generation.maxOutputTokens = room + CLAUDE_ROOM;
      return { include_thoughts: true, thinking_budget: room };
    }
    case "levels":
      return tiered ? { includeThoughts: true } : { includeThoughts: true, thinkingLevel: level === "high" ? "high" : "low" };
    case "budget":
      return { includeThoughts: true, thinkingBudget: budget(name, level) };
    default:
      return undefined;
  }
}

export function shape({ body, effort }) {
  const inner = body.request ?? {};
  const name = String(body.model ?? "").split(":").pop() ?? "";
  const generation = { maxOutputTokens: Number(inner.generationConfig?.maxOutputTokens ?? 0) };
  const thought = thinking(name, suffixOf(name) ?? effort, generation);
  inner.generationConfig = thought === undefined ? generation : { ...generation, thinkingConfig: thought };
  if (family(name) === "claude" && inner.tools !== undefined) inner.toolConfig = { functionCallingConfig: { mode: "VALIDATED" } };
  body.request = inner;
  body.userAgent = "antigravity";
  body.requestType = "agent";
  body.requestId = `agent-${body.requestId}`;
  return body;
}

export const retriesAfterRefresh =({ status }: { status: number }) => status === 401;

export function catalog({ config }: { config: Config }, ctx) {
  const root = config.base_url ? config.base_url.replace(/\/+$/, "") : ctx.config.api;
  return {
    url: `${root}/v1internal:fetchAvailableModels`,
    headers: signed(ctx, config.auth?.access_token ?? ""),
    body: JSON.stringify({ project: config.auth?.account_id ?? "" }),
  };
}

function modelOf(ctx, given: string, item): Model | null {
  const id = given.trim().replace(/^(models\/)+/, "");
  if (id === "" || id.startsWith("chat_") || id.startsWith("tab_") || id.includes("image")) return null;
  const name = typeof item?.displayName === "string" ? item.displayName.trim() : "";
  const found = option(ctx, id, name === "" ? id : name);
  found.window = positive(item?.maxTokens) ?? positive(item?.inputTokenLimit);
  found.capabilities = capabilities({
    reasoning: !id.startsWith("gpt-oss"),
    max_output_tokens: positive(item?.maxOutputTokens) ?? positive(item?.outputTokenLimit),
  });
  return found;
}

export function page({ value }, ctx) {
  const listed = value?.models;
  let models: (Model | null)[] = [];
  if (Array.isArray(listed)) {
    models = listed.map((item) => {
      const id = typeof item?.id === "string" ? item.id : typeof item?.name === "string" ? item.name : undefined;
      return id === undefined ? null : modelOf(ctx, id, item);
    });
  } else if (listed !== null && typeof listed === "object") {
    models = Object.entries(listed).map(([id, item]) => modelOf(ctx, id, item));
  }
  return { models: models.filter((found) => found !== null), next: null };
}

export function finish({ models }: { models: Model[] }, ctx) {
  return models.length > 0 ? models : words(ctx.config.fallback_models).map((id) => modelOf(ctx, id, null)).filter((found) => found !== null);
}

const secretPairs = (ctx): [string, string][] => [
  ["client_id", ctx.config.client_id],
  ["client_secret", ctx.config.client_secret],
];

export function refresh({ previous }: { previous: Tokens }, ctx) {
  return formRequest(ctx, ctx.config.token_url, [...secretPairs(ctx), ["grant_type", "refresh_token"], ["refresh_token", previous.refresh_token ?? ""]]);
}

export function tokens(input, ctx) {
  const fresh = standard(ctx, input);
  if (!fresh.refresh_token) unauthorized("Google returned no refresh token");
  return { ...fresh, account_id: input.previous?.account_id ?? "", email: fresh.email ?? input.previous?.email ?? null };
}

function authorize(ctx, challenge: string, state: string, redirect: string) {
  const query = form(ctx, [
    ["client_id", ctx.config.client_id],
    ["response_type", "code"],
    ["redirect_uri", redirect],
    ["scope", ctx.config.scopes],
    ["code_challenge", challenge],
    ["code_challenge_method", "S256"],
    ["state", state],
    ["access_type", "offline"],
    ["prompt", "consent"],
  ]);
  return `${ctx.config.authorize_url}?${query}`;
}

function assist(ctx, accessToken: string, method: string, body: unknown) {
  const headers: [string, string][] = [
    ["authorization", `Bearer ${accessToken}`],
    ["content-type", "application/json"],
    ["user-agent", userAgent(ctx)],
  ];
  try {
    const reached = ctx.http.post({ url: `${ctx.config.api}/v1internal:${method}`, headers, body: JSON.stringify(body) });
    return reached.status === 200 ? parsed(reached.body) : undefined;
  } catch (_) {
    return undefined;
  }
}

function projectOf(value): string | undefined {
  const project = value?.cloudaicompanionProject;
  const id = typeof project === "string" ? project : typeof project?.id === "string" ? project.id : "";
  return id.trim() === "" ? undefined : id.trim();
}

function tierOf(value): string {
  const tiers = Array.isArray(value?.allowedTiers) ? value.allowedTiers : [];
  const id = (tier) => (typeof tier?.id === "string" && tier.id !== "" ? tier.id : undefined);
  return id(tiers.find((tier) => tier?.isDefault === true)) ?? id(tiers[0]) ?? id(value?.currentTier) ?? FREE_TIER;
}

function project(ctx, accessToken: string): string {
  const loaded = assist(ctx, accessToken, "loadCodeAssist", { metadata: { ideType: "ANTIGRAVITY" } });
  const found = projectOf(loaded);
  if (found !== undefined) return found;
  const tier = loaded === undefined ? FREE_TIER : tierOf(loaded);
  const body = { tier_id: tier, metadata: { ide_type: "ANTIGRAVITY", ide_version: ctx.config.version, ide_name: "antigravity" } };
  for (let attempt = 0; attempt < ONBOARD_ATTEMPTS; attempt += 1) {
    const answer = assist(ctx, accessToken, "onboardUser", body);
    if (answer === undefined) break;
    if (answer.done === true) return projectOf(answer.response) ?? ctx.config.default_project;
    if (attempt + 1 < ONBOARD_ATTEMPTS) ctx.clock.sleep(ONBOARD_WAIT);
  }
  return ctx.config.default_project;
}

function emailOf(ctx, accessToken: string): string | null {
  try {
    const reached = ctx.http.get({ url: ctx.config.userinfo_url, headers: [["authorization", `Bearer ${accessToken}`]] });
    const email = reached.status === 200 ? parsed(reached.body)?.email : undefined;
    return typeof email === "string" && email.trim().includes("@") && email.trim().length <= 254 ? email.trim() : null;
  } catch (_) {
    return null;
  }
}

export function signIn(_: unknown, ctx) {
  const signed = browser(ctx, {
    callback: callback(ctx.config.callback_port, ctx.config.callback_path),
    stateBytes: 32,
    authorize: (challenge, state, redirect) => authorize(ctx, challenge, state, redirect),
    exchange: (code, _state, verifier, redirect) =>
      formRequest(ctx, ctx.config.token_url, [...secretPairs(ctx), ["code", code], ["grant_type", "authorization_code"], ["redirect_uri", redirect], ["code_verifier", verifier]]),
    tokens,
  });
  const account_id = project(ctx, signed.access_token);
  return { auth: { ...signed, account_id, email: signed.email ?? emailOf(ctx, signed.access_token) } };
}
