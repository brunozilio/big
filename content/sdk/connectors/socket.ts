import { ActionError, classify } from "./index.ts";

export type Frame = { text: string } | { binary: string } | { base64: string };
export type Received = { text?: string; base64?: string; closed?: boolean; timeout?: boolean };
export type Message = { headers: Record<string, string | number | boolean>; payload: string };

export function open(ctx, url: string, headers: Record<string, string> = {}): number {
  const opened = ctx.socket.open({ url, headers });
  if (opened?.status === 101 && Number.isInteger(opened.socket)) return opened.socket;
  const status = Number(opened?.status ?? 0);
  const kind = classify(status, {}, Math.floor(Date.now() / 1000)) ?? "permanent";
  throw new ActionError(kind, `the service refused the socket (${status}): ${String(opened?.body ?? "").slice(0, 300)}`);
}

export const send = (ctx, socket: number, frames: Frame[]) => ctx.socket.send({ socket, frames });

export const receive = (ctx, socket: number, timeoutMs: number): Received => ctx.socket.receive({ socket, timeoutMs });

export const close = (ctx, socket: number) => ctx.socket.close({ socket });

function bytesOf(ctx, base64: string): number[] {
  const hex: string = ctx.encoding.convert({ data: base64, from: "base64", to: "hex" });
  const bytes: number[] = [];
  for (let at = 0; at < hex.length; at += 2) bytes.push(Number.parseInt(hex.slice(at, at + 2), 16));
  return bytes;
}

const hexOf = (bytes: number[]) => bytes.map((byte) => byte.toString(16).padStart(2, "0")).join("");

const textOf = (ctx, bytes: number[]) => (bytes.length === 0 ? "" : ctx.encoding.convert({ data: hexOf(bytes), from: "hex", to: "text" }));

const number = (bytes: number[], at: number, size: number) => bytes.slice(at, at + size).reduce((value, byte) => value * 256 + byte, 0);

const SIZES: Record<number, number> = { 2: 1, 3: 2, 4: 4, 5: 8, 8: 8, 9: 16 };

function header(ctx, bytes: number[], at: number): [string, string | number | boolean, number] {
  const nameLength = bytes[at];
  const name = textOf(ctx, bytes.slice(at + 1, at + 1 + nameLength));
  let next = at + 1 + nameLength;
  const kind = bytes[next];
  next += 1;
  if (kind === 0 || kind === 1) return [name, kind === 0, next];
  if (kind === 6 || kind === 7) {
    const length = number(bytes, next, 2);
    const raw = bytes.slice(next + 2, next + 2 + length);
    return [name, kind === 7 ? textOf(ctx, raw) : hexOf(raw), next + 2 + length];
  }
  const size = SIZES[kind];
  if (size === undefined) throw new ActionError("permanent", `the event stream has a header of unknown type ${kind}`);
  return [name, size <= 4 ? number(bytes, next, size) : hexOf(bytes.slice(next, next + size)), next + size];
}

export function eventStream(ctx, base64: string): Message {
  const bytes = bytesOf(ctx, base64);
  const total = number(bytes, 0, 4);
  const headersLength = number(bytes, 4, 4);
  if (bytes.length < 16 || total !== bytes.length || 12 + headersLength > total - 4) {
    throw new ActionError("permanent", "the event stream message is cut or malformed");
  }
  const headers: Record<string, string | number | boolean> = {};
  let at = 12;
  while (at < 12 + headersLength) {
    const [name, value, next] = header(ctx, bytes, at);
    headers[name] = value;
    at = next;
  }
  return { headers, payload: textOf(ctx, bytes.slice(12 + headersLength, total - 4)) };
}
