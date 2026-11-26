import { ActionError } from "sdk/connectors/index.ts";
import { close, eventStream, open, receive, send } from "sdk/connectors/socket.ts";
import type { Held } from "./login.ts";

export type Asked = { question: string; context?: string; thread?: string; mode?: string };
export type Source = { title: string; url: string };

const LIMITED = new Set(["throttled", "throttling", "usage_limit", "rate_limited", "service_unavailable"]);

function uuid(ctx): string {
  const hex: string = ctx.crypto.random({ bytes: 16, output: "hex" });
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-4${hex.slice(13, 16)}-a${hex.slice(17, 20)}-${hex.slice(20, 32)}`;
}

const encoded = (ctx, inner: unknown) => ctx.encoding.convert({ data: JSON.stringify(inner), from: "text", to: "base64" });

const outgoing = (ctx, inner: unknown) => ({ binary: JSON.stringify({ eventType: "rawInputEvent", payload: { payload: encoded(ctx, inner) } }) });

function address(ctx, held: Held): string {
  const origin = held.tenant_url.replace(/^http/, "ws").replace(/^(wss?:\/\/[^/]+).*$/, "$1");
  const query = [
    ["surfaceType", ctx.config.surface_type],
    ["token", held.token],
    ["workingSessionId", uuid(ctx)],
  ]
    .map(([name, value]) => `${name}=${encodeURIComponent(value)}`)
    .join("&");
  return `${origin}${ctx.config.chat_path}?${query}`;
}

function said(asked: Asked): string {
  const context = typeof asked.context === "string" ? asked.context.trim() : "";
  return context === "" ? asked.question.trim() : `<context>\n${context}\n</context>\n\n${asked.question.trim()}`;
}

function modeOf(ctx, wanted: unknown): string {
  const modes = String(ctx.config.modes).split(/\s+/);
  return typeof wanted === "string" && modes.includes(wanted) ? wanted : ctx.config.default_mode;
}

function inner(ctx, payload: string) {
  try {
    const outer = JSON.parse(payload);
    if (typeof outer?.payload !== "string") return outer;
    return JSON.parse(ctx.encoding.convert({ data: outer.payload, from: "base64", to: "text" }));
  } catch (_) {
    return null;
  }
}

function failed(code: string, message: string): never {
  const known = code.toLowerCase();
  const told = `Amazon Quick: ${message === "" ? code : message}`;
  if (LIMITED.has(known)) throw new ActionError("rate_limited", told, 60_000);
  if (known === "unauthorized" || known === "accessdenied" || known === "expired_token") throw new ActionError("auth", told);
  throw new ActionError("permanent", told);
}

export function sourced(text: string): { answer: string; sources: Source[] } {
  const block = /<sources>([\s\S]*?)<\/sources>/g;
  const sources: Source[] = [];
  for (const found of text.matchAll(block)) {
    for (const link of found[1].matchAll(/\[([^\]]+)\]\(([^)\s]+)\)/g)) sources.push({ title: link[1], url: link[2] });
  }
  return { answer: text.replace(block, "").trim(), sources };
}

type Turn = { text: string; model: string; done: boolean };

function heard(ctx, socket: number, found, turn: Turn) {
  const payload = found?.payload ?? {};
  if (found?.type === "tool_call") {
    const refused = { type: "tool_result", payload: { tool_call_id: payload.tool_call_id, result: null, error: "this client runs no tools" } };
    send(ctx, socket, [outgoing(ctx, refused)]);
    return;
  }
  if (found?.scope !== undefined && found.scope !== "chat") return;
  if (found?.type === "text_delta" && typeof payload.delta === "string") turn.text += payload.delta;
  if (found?.type === "turn_usage" && typeof payload.served_model_id === "string") turn.model = payload.served_model_id;
  if (found?.type === "error") failed(String(payload.error_code || payload.error_type || payload.code || "error"), String(payload.message ?? ""));
  if (found?.type === "guardrail_blocked") failed("guardrail_blocked", String(payload.message ?? "the answer was blocked by a guardrail"));
  if (found?.type === "message_complete" && payload.continuation_expected !== true) turn.done = true;
}

function listen(ctx, socket: number, turn: Turn) {
  const deadline = Date.now() + Number.parseInt(ctx.config.answer_seconds, 10) * 1000;
  while (!turn.done) {
    const left = deadline - Date.now();
    if (left <= 0) throw new ActionError("transient", "Amazon Quick did not finish the answer in time");
    const received = receive(ctx, socket, left);
    if (received.timeout) continue;
    if (received.closed) throw new ActionError("transient", "Amazon Quick closed the conversation before answering");
    const message = received.base64 ? eventStream(ctx, received.base64) : { headers: {}, payload: received.text ?? "" };
    const kind = message.headers[":message-type"];
    if (kind === "error" || kind === "exception") failed(String(message.headers[":error-code"] ?? message.headers[":exception-type"] ?? "error"), String(message.headers[":error-message"] ?? message.payload));
    heard(ctx, socket, inner(ctx, message.payload), turn);
  }
}

export function ask(ctx, held: Held, asked: Asked) {
  if (typeof asked?.question !== "string" || asked.question.trim() === "") throw new ActionError("permanent", "ask a question");
  const thread = typeof asked.thread === "string" && asked.thread !== "" ? asked.thread : uuid(ctx);
  const connect = { type: "session_connect", payload: { surfaceType: ctx.config.surface_type, appVersion: ctx.config.app_version } };
  const message = {
    type: "user_message",
    payload: {
      thread_id: thread,
      message_id: uuid(ctx),
      text: said(asked),
      agent_mode: modeOf(ctx, asked.mode),
      user_timezone: ctx.config.timezone,
      user_locale: ctx.config.locale,
    },
  };
  const socket = open(ctx, address(ctx, held));
  const turn: Turn = { text: "", model: "", done: false };
  try {
    send(ctx, socket, [outgoing(ctx, connect), outgoing(ctx, message)]);
    listen(ctx, socket, turn);
  } finally {
    close(ctx, socket);
  }
  const { answer, sources } = sourced(turn.text);
  return turn.model === "" ? { answer, thread, sources } : { answer, thread, sources, model: turn.model };
}
