import { type Config, type Tokens, account, invalid, refreshable, regional, unauthorized } from "sdk/providers/index.ts";
import { pickEffort, page as pageOf } from "./models.ts";
import { tokens as socialTokens, refresh as socialRefresh } from "./auth.ts";
import { tokens as organizationTokens, refresh as organizationRefresh, clientOf } from "./idc.ts";
import { windows as windowsOf } from "./limits.ts";
import { signIn as signInWith } from "./sign_in.ts";

export const CHAT = "AmazonCodeWhispererStreamingService.GenerateAssistantResponse";
export const MODELS = "KiroControlPlaneBearerService.ListAvailableModels";
export const SEARCH = "AmazonCodeWhispererStreamingService.InvokeMCP";
export const USAGE = "AmazonCodeWhispererService.GetUsageLimits";
export const PROFILES = "AmazonCodeWhispererService.ListAvailableProfiles";
const ID = "kiro-subscription";

export function provider() {
  return {
    id: ID,
    name: "Kiro Subscription",
    label: "Kiro",
    logo: "logo.svg",
    vendor: "kiro",
    smallModel: "claude-haiku-4.5",
    access: "browser",
    rank: 70,
    slot: "0x7072_6f76_6964_6572_0000_0000_0000_0007",
    window: 200_000,
    wire: "event_stream",
    path: "/",
  };
}

const regionName = (text: string) => text !== "" && /^[a-z0-9-]+$/.test(text);

export function region(ctx, profileArn: string): string {
  const found = profileArn.split(":")[3] ?? "";
  return regionName(found) ? found : ctx.config.region;
}

export const runtime = (ctx, profileArn: string) => regional(ctx.config.runtime_url, region(ctx, profileArn));

export const management = (ctx, profileArn: string) => regional(ctx.config.management_url, region(ctx, profileArn));

export function userAgent(ctx, target: string): string {
  return target.startsWith("AmazonCodeWhispererStreamingService.") ? ctx.config.user_agent_streaming : ctx.config.user_agent_runtime;
}

export function signed(ctx, accessToken: string, target: string): [string, string][] {
  const agent = userAgent(ctx, target);
  return [
    ["authorization", `Bearer ${accessToken}`],
    ["content-type", "application/x-amz-json-1.0"],
    ["x-amz-target", target],
    ["user-agent", agent],
    ["x-amz-user-agent", agent],
  ];
}

export function check({ config }: { config: Config }) {
  if (!refreshable(config).account_id) unauthorized("Kiro profile ARN is missing");
}

export const base = ({ config }: { config: Config }, ctx) => runtime(ctx, account(config));

export const headers = ({ config }: { config: Config }, ctx) => signed(ctx, config.auth?.access_token ?? "", CHAT);

export function bind({ config, body }: { config: Config; body: Record<string, unknown> }) {
  if (config.auth) body.profileArn = account(config);
  return body;
}

export const retriesAfterRefresh = ({ status }: { status: number }) => status === 401 || status === 403;

export function shape({ model, body, effort }, ctx) {
  const prefix = `${ID}:`;
  const strip = (message) => {
    const input = message?.userInputMessage;
    if (typeof input?.modelId !== "string") return;
    input.modelId = input.modelId.startsWith(prefix) ? input.modelId.slice(prefix.length) : input.modelId;
    input.origin = ctx.config.origin;
  };
  const state = body.conversationState ?? {};
  strip(state.currentMessage);
  if (Array.isArray(state.history)) state.history.forEach(strip);
  delete body.output_config;
  return pickEffort(model, body, effort);
}

export function catalog({ config, after }: { config: Config; after: string }, ctx) {
  const arn = account(config);
  const body: Record<string, unknown> = { origin: ctx.config.origin, profileArn: arn };
  if (after !== "") body.nextToken = after;
  return { url: `${management(ctx, arn)}/`, headers: signed(ctx, config.auth?.access_token ?? "", MODELS), body: JSON.stringify(body) };
}

export const page = ({ value }, ctx) => pageOf(ctx, value) ?? invalid("model catalog has no model array");

export function search({ config, query }: { config: Config; query: string }, ctx) {
  const body = { jsonrpc: "2.0", id: "1", method: "tools/call", params: { name: "web_search", arguments: { query } } };
  return { url: `${runtime(ctx, account(config))}/`, headers: signed(ctx, config.auth?.access_token ?? "", SEARCH), body: JSON.stringify(body) };
}

export function hits({ value }) {
  if (value?.error !== undefined && value.error !== null) {
    invalid(`Kiro's web search refused: ${typeof value.error.message === "string" ? value.error.message : JSON.stringify(value.error)}`);
  }
  const result = value?.result ?? {};
  const text = (Array.isArray(result.content) ? result.content : []).map((part) => (typeof part?.text === "string" ? part.text : "")).join("");
  if (result.isError === true) invalid(`Kiro's web search failed: ${text}`);
  let found;
  try {
    found = JSON.parse(text);
  } catch (_) {
    invalid("Kiro's web search answered without results");
  }
  if (typeof found?.error === "string") invalid(`Kiro's web search failed: ${found.error}`);
  return (Array.isArray(found?.results) ? found.results : [])
    .filter((item) => typeof item?.url === "string" && item.url !== "")
    .map((item) => ({ title: typeof item.title === "string" ? item.title : item.url, url: item.url, snippet: typeof item.snippet === "string" ? item.snippet : "" }));
}

export function limits({ config }: { config: Config }, ctx) {
  const arn = account(config);
  return {
    url: regional(ctx.config.q_url, region(ctx, arn)),
    headers: [...signed(ctx, config.auth?.access_token ?? "", USAGE).filter(([name]) => name !== "accept"), ["accept", "application/json"]],
    body: JSON.stringify({ origin: ctx.config.origin, profileArn: arn }),
  };
}

export const windows = ({ value, language }, ctx) => windowsOf(ctx, value, language);

export function refresh({ previous }: { previous: Tokens }, ctx) {
  const client = clientOf(previous);
  return client ? organizationRefresh(ctx, client, previous) : socialRefresh(ctx, previous);
}

export function tokens(input, ctx) {
  const client = input.previous ? clientOf(input.previous) : undefined;
  return client ? organizationTokens(ctx, client, input) : socialTokens(ctx, input);
}

export const signIn = (_: unknown, ctx) => ({ auth: signInWith(ctx) });
