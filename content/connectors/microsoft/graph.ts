import { type Creds, type Request, bearer, checked, fail, json, listIn, send } from "sdk/connectors/index.ts";

type Ctx = { config: { graph: string } } & Record<string, any>;

export type Api = { ctx: Ctx; base: string; token: string };

const MOST_DOWNLOAD = 512 * 1024;

export function apiOf(ctx: Ctx, creds: Creds): Api {
  const base = String(ctx.config.graph || "").replace(/\/$/, "");
  if (base === "") fail("permanent", "the Microsoft Graph endpoint is not configured");
  return { ctx, base, token: creds.token };
}

export function call(api: Api, request: Request) {
  const headers = { ...bearer(api.token), accept: "application/json", ...(request.headers ?? {}) };
  const url = request.url.startsWith("http") ? request.url : `${api.base}${request.url}`;
  return checked(send(api.ctx, { ...request, url, headers }));
}

export function get(api: Api, path: string) {
  return json(call(api, { method: "GET", url: path }));
}

export function post(api: Api, path: string, body: unknown) {
  const answer = call(api, { method: "POST", url: path, headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
  return answer.body === "" ? {} : json(answer);
}

export const items = (answer: unknown): any[] => listIn((answer as any)?.value);

export function download(api: Api, path: string): { mime: string; base64: string; truncated: boolean } {
  const answer = call(api, { method: "GET", url: path, binary: true });
  const base64 = typeof answer.base64 === "string" ? answer.base64 : "";
  const bytes = Math.floor((base64.length * 3) / 4);
  const mime = answer.headers?.["content-type"] ?? answer.headers?.["Content-Type"] ?? "application/octet-stream";
  if (bytes > MOST_DOWNLOAD) return { mime, base64: "", truncated: true };
  return { mime, base64, truncated: false };
}

export const quote = (value: string) => value.replace(/'/g, "''");
