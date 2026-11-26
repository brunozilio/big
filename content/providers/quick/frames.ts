import { parsed } from "sdk/providers/index.ts";

type Turn = { role: string; text: string };

const LIMITED = new Set(["throttled", "throttling", "usage_limit", "rate_limited", "service_unavailable"]);
const TOO_LONG = new Set(["context_overflow", "context_length_exceeded", "too_long"]);

const encoded = (ctx, inner: unknown) => ctx.encoding.convert({ data: JSON.stringify(inner), from: "text", to: "base64" });

export const outgoing = (ctx, inner: unknown) => ({ binary: JSON.stringify({ eventType: "rawInputEvent", payload: { payload: encoded(ctx, inner) } }) });

function said(system: string, messages: Turn[]): string {
  const turns = messages.filter((turn) => typeof turn?.text === "string" && turn.text.trim() !== "");
  const last = turns.length > 0 && turns[turns.length - 1].role === "user" ? turns[turns.length - 1].text : "";
  const earlier = last === "" ? turns : turns.slice(0, -1);
  const parts: string[] = [];
  if (system.trim() !== "") parts.push(`<instructions>\n${system.trim()}\n</instructions>`);
  if (earlier.length > 0) {
    const history = earlier.map((turn) => `<${turn.role}>\n${turn.text.trim()}\n</${turn.role}>`).join("\n");
    parts.push(`<conversation>\n${history}\n</conversation>`);
  }
  parts.push(last.trim());
  return parts.filter((part) => part !== "").join("\n\n");
}

export function modeOf(model: unknown, modes: string[], fallback: string): string {
  const id = typeof model === "string" ? model.slice(model.lastIndexOf(":") + 1) : "";
  return modes.includes(id) ? id : fallback;
}

function continued(body, messages: Turn[]): { thread: string; fresh: Turn[] } | undefined {
  const thread = body?.kept?.value?.thread;
  const already = body?.kept?.sent;
  if (typeof thread !== "string" || thread === "" || !Number.isInteger(already)) return undefined;
  if (already >= messages.length || messages[messages.length - 1]?.role !== "user") return undefined;
  return { thread, fresh: messages.slice(already) };
}

export function sent(ctx, body, mode: string) {
  const messages: Turn[] = Array.isArray(body?.messages) ? body.messages : [];
  const going = continued(body, messages);
  const connect = { type: "session_connect", payload: { surfaceType: ctx.config.surface_type, appVersion: ctx.config.app_version } };
  const payload: Record<string, unknown> = {
    thread_id: going?.thread ?? ctx.crypto.uuid(),
    message_id: ctx.crypto.uuid(),
    text: going ? said("", going.fresh) : said(typeof body?.system === "string" ? body.system : "", messages),
    agent_mode: mode,
    web_search_enabled: false,
    user_timezone: ctx.config.timezone,
    user_locale: ctx.config.locale,
  };
  return { send: [outgoing(ctx, connect), outgoing(ctx, { type: "user_message", payload })] };
}

function inner(ctx, payload: string) {
  const outer = parsed(payload);
  if (typeof outer?.payload !== "string") return outer;
  return parsed(ctx.encoding.convert({ data: outer.payload, from: "base64", to: "text" }));
}

function failure(code: string, message: string) {
  const known = code.toLowerCase();
  const kind = LIMITED.has(known) ? "overloaded" : TOO_LONG.has(known) ? "too_long" : known === "unauthorized" ? "unauthorized" : "provider";
  return { kind, message: message === "" ? code : message };
}

const words = (value: unknown) => (typeof value === "string" ? value : "");

function usage(payload) {
  const count = (name: string) => (Number.isInteger(payload?.[name]) && payload[name] > 0 ? payload[name] : 0);
  return { input: count("input_tokens"), output: count("output_tokens"), cache_read: count("cache_read_tokens"), cache_write: count("cache_write_tokens") };
}

function event(state, found) {
  const payload = found?.payload ?? {};
  const thread = typeof found?.thread_id === "string" && found.thread_id !== "" ? found.thread_id : state?.thread;
  const kept = { state: thread ? { thread } : state };
  switch (found?.type) {
    case "text_delta":
      return { ...kept, text: words(payload.delta) };
    case "thinking_delta":
      return { ...kept, thinking: words(payload.delta) };
    case "turn_usage":
      return { ...kept, usage: usage(payload), served: words(payload.served_model_id) };
    case "message_complete":
      if (payload.continuation_expected === true) return kept;
      return thread ? { ...kept, done: true, keep: { thread } } : { ...kept, done: true };
    case "error":
      return { ...kept, error: failure(words(payload.error_code || payload.error_type || payload.code), words(payload.message)) };
    case "guardrail_blocked":
      return { ...kept, error: failure("guardrail_blocked", words(payload.message) || "the answer was blocked by a guardrail") };
    default:
      return kept;
  }
}

export function frame(ctx, state, received) {
  const headers = received?.headers ?? {};
  const kind = headers[":message-type"];
  if (kind === "error" || kind === "exception") {
    return { state, error: failure(words(headers[":error-code"] ?? headers[":exception-type"]), words(headers[":error-message"]) || words(received?.payload)) };
  }
  const found = inner(ctx, words(received?.payload));
  if (found?.scope !== undefined && found.scope !== "chat") return { state };
  return event(state, found);
}
