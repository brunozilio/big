export type Language = "en" | "es" | "pt-BR";

export type ErrorKind = "transient" | "auth" | "conflict" | "permanent" | "rate_limited";

export type Creds = { connection: string; token: string; settings: Record<string, unknown> };

export type Field = { name: string; label: string; help?: string; secret?: boolean; setting?: boolean; value?: string };

export type Response = { status: number; headers: Record<string, string>; body: string; base64?: string };

export type Request = {
  method?: string;
  url: string;
  headers?: Record<string, string>;
  body?: string;
  base64?: string;
  binary?: boolean;
};

export const RECONNECT = "the access could not be renewed; reconnect the account";
export const DEFAULT_RETRY_AFTER_MS = 60_000;

export function pick<T>(language: Language | string | undefined, english: T, spanish: T, portuguese: T): T {
  if (language === "en") return english;
  if (language === "es") return spanish;
  return portuguese;
}

export class ActionError extends Error {
  class: Record<string, unknown>;
  constructor(kind: ErrorKind, message: string, retryAfterMs?: number) {
    super(message);
    this.class = kind === "rate_limited" ? { kind, retry_after: retryAfterMs ?? DEFAULT_RETRY_AFTER_MS } : { kind };
  }
}

export const fail = (kind: ErrorKind, message: string, retryAfterMs?: number): never => {
  throw new ActionError(kind, message, retryAfterMs);
};

export const missing = (argument: string): never => fail("permanent", `the argument \`${argument}\` is missing or has the wrong type`);

export const unknownAction = (action: string): never => fail("permanent", `the action \`${action}\` is not offered here`);

export const decode = (what: string): never => fail("permanent", `the answer could not be read: ${what}`);

function header(headers: Record<string, string>, name: string): string | undefined {
  const found = Object.keys(headers).find((key) => key.toLowerCase() === name);
  return found === undefined ? undefined : String(headers[found]).trim();
}

export function retryAfterMs(headers: Record<string, string>, nowSecond: number): number | undefined {
  const seconds = Number.parseInt(header(headers, "retry-after") ?? "", 10);
  if (Number.isFinite(seconds) && String(seconds) === header(headers, "retry-after")) return seconds * 1000;
  const exhausted = header(headers, "x-ratelimit-remaining") === "0";
  const reset = Number.parseInt(header(headers, "x-ratelimit-reset") ?? "", 10);
  if (exhausted && Number.isFinite(reset)) return Math.max(1, reset - nowSecond) * 1000;
  if (exhausted) return DEFAULT_RETRY_AFTER_MS;
  return undefined;
}

export function classify(status: number, headers: Record<string, string>, nowSecond: number): ErrorKind | undefined {
  if (status >= 200 && status < 400) return undefined;
  const limited = retryAfterMs(headers, nowSecond);
  if (status === 429) return "rate_limited";
  if (status === 403 && limited !== undefined) return "rate_limited";
  if (status === 401 || status === 403) return "auth";
  if (status === 409 || status === 412) return "conflict";
  if (status === 408 || (status >= 500 && status <= 599)) return "transient";
  return "permanent";
}

export function send(ctx, request: Request): Response {
  const answer = ctx.http(request);
  if (answer.failed) throw new ActionError(answer.failed.class?.kind ?? "transient", answer.failed.message, answer.failed.class?.retry_after);
  return answer;
}

export function checked(answer: Response, nowSecond: number = Math.floor(Date.now() / 1000)): Response {
  const kind = classify(answer.status, answer.headers, nowSecond);
  if (kind === undefined) return answer;
  const body = [...answer.body].slice(0, 500).join("");
  throw new ActionError(kind, `the service answered ${answer.status}: ${body}`, retryAfterMs(answer.headers, nowSecond));
}

export function fetch(ctx, request: Request): Response {
  return checked(send(ctx, request));
}

export function json(answer: Response) {
  if (answer.body === "") return null;
  try {
    return JSON.parse(answer.body);
  } catch (error) {
    return decode(String(error));
  }
}

export function fetchJson(ctx, request: Request) {
  return json(fetch(ctx, request));
}

export const bearer = (token: string) => ({ authorization: `Bearer ${token}` });

export const authorized = (token: string) => ({ authorization: token.startsWith("Basic ") ? token : `Bearer ${token}` });

export function form(fields: [string, string][]): string {
  return fields.map(([name, value]) => `${encode(name)}=${encode(value)}`).join("&");
}

export const encode = (text: string) =>
  encodeURIComponent(text)
    .replace(/[!'()~]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`)
    .replace(/%20/g, "+");

export const query = form;

export function textAt(value: unknown, path: string[]): string {
  let found: any = value;
  for (const key of path) found = found == null ? undefined : found[key];
  return typeof found === "string" ? found : "";
}

export function idAt(value: unknown, path: string[]): string {
  let found: any = value;
  for (const key of path) found = found == null ? undefined : found[key];
  if (typeof found === "string") return found;
  if (typeof found === "number") return String(found);
  return "";
}

export const blocked = (reason: string): never => fail("permanent", `blocked: ${reason}`);

export const NO_PROJECT = "Choose the project.";

export const useApiToken = (language: Language) => pick(language, "Use an API token", "Usar un API token", "Usar API token");

export function text(args: Record<string, unknown>, name: string): string {
  const value = args?.[name];
  return typeof value === "string" && value !== "" ? value : missing(name);
}

export function optionalText(args: Record<string, unknown>, name: string): string | undefined {
  const value = args?.[name];
  return typeof value === "string" && value !== "" ? value : undefined;
}

export function optionalNumber(args: Record<string, unknown>, name: string): number | undefined {
  const value = args?.[name];
  if (typeof value === "number" && Number.isInteger(value) && value >= 0) return value;
  if (typeof value === "string" && /^\d+$/.test(value)) return Number(value);
  return undefined;
}

export function number(args: Record<string, unknown>, name: string): number {
  return optionalNumber(args, name) ?? missing(name);
}

export function optionalFlag(args: Record<string, unknown>, name: string): boolean | undefined {
  const value = args?.[name];
  return typeof value === "boolean" ? value : undefined;
}

export function flag(args: Record<string, unknown>, name: string): boolean {
  return optionalFlag(args, name) ?? missing(name);
}

export function listOf(args: Record<string, unknown>, name: string): unknown[] {
  const value = args?.[name];
  return Array.isArray(value) ? value : [];
}

export function setting(creds: Creds, name: string): string | undefined {
  const value = creds.settings?.[name];
  return typeof value === "string" && value !== "" ? value : undefined;
}

export function settingsList(creds: Creds, name: string): string[] {
  const value = creds.settings?.[name];
  return Array.isArray(value) ? value.filter((item) => typeof item === "string") : [];
}

export const connect = (language: Language) => pick(language, "Connect", "Conectar", "Conectar");

export function tokenField(name: string, label: string, help: string, secret: boolean): Field {
  return { name, label, help, secret, setting: false, value: "" };
}

export function settingField(name: string, label: string, help: string, value = ""): Field {
  return { name, label, help, secret: false, setting: true, value };
}

export function tokenAlternative(language: Language, key: string, label: string, site: string, fields: Field[], form: "bearer" | "basic" | "json") {
  return { key, label, button: connect(language), site, auth: { kind: "token", fields, form } };
}

export function siteOf(url: string): string {
  const match = /^[a-z][a-z0-9+.-]*:\/\/([^/?#]+)/i.exec(url.trim());
  if (!match) return url;
  const host = match[1].replace(/^[^@]*@/, "").replace(/:\d+$/, "").toLowerCase();
  return host.replace(/^www\./, "");
}

export const formComponent = encode;

export function textOf(value: unknown, path: string[]): string {
  let found: any = value;
  for (const key of path) found = found == null ? undefined : found[key];
  return typeof found === "string" ? found : "";
}

export const jsonText = (value: unknown): string => JSON.stringify(value === undefined ? null : value);

export const listIn = (value: unknown): any[] => (Array.isArray(value) ? value : []);

export function timestamp(text: string): number | undefined {
  if (typeof text !== "string" || text === "") return undefined;
  const at = Date.parse(text.replace(/([+-]\d\d)(\d\d)$/, "$1:$2"));
  return Number.isNaN(at) ? undefined : at;
}

export function iso(ms: number): string {
  return new Date(ms).toISOString().replace(".000Z", "Z");
}

export function expiresAt(nowSecond: number, seconds: number | undefined): string | null {
  if (seconds === undefined || seconds === null || !Number.isFinite(seconds)) return null;
  return new Date((nowSecond + seconds) * 1000).toISOString();
}

export function classOf(error): string {
  if (error?.class?.kind) return error.class.kind;
  try {
    return JSON.parse(String(error?.message ?? error))?.class?.kind ?? "permanent";
  } catch (_) {
    return "permanent";
  }
}
