import { ActionError, type Language } from "sdk/connectors/index.ts";

const AUTH = "/sonarlint/auth";
const STATUS = "/sonarlint/api/status";
const TOKEN = "/sonarlint/api/token";

type Request = { method: string; path: string; body: unknown };

type Ctx = { config: { url: string; loopback_from: string; loopback_to: string }; brand: { NAME: string } } & Record<string, any>;

type Entered = { base: string };

const number = (text: string) => Number.parseInt(text, 10);

export function signInWith(baseOf: (entered: string | undefined, fallback: string) => string, fieldsOf: (ctx: Ctx, language: Language) => unknown[]) {
  return {
    name: ({ state }: { state: Partial<Entered> }, ctx: Ctx) => state?.base ?? ctx.config.url,
    port: (_: unknown, ctx: Ctx) => [number(ctx.config.loopback_from), number(ctx.config.loopback_to)],
    fields: ({ language }: { language: Language }, ctx: Ctx) => fieldsOf(ctx, language),
    forFields: ({ fields }: { fields: Record<string, string> }, ctx: Ctx) => ({ state: { base: baseOf(fields?.url, ctx.config.url) } }),
    begin({ state, callback }: { state: Partial<Entered>; callback: number | null }, ctx: Ctx) {
      if (!callback) throw new ActionError("permanent", "the sign-in needs a port on this computer to receive the token");
      const base = state?.base ?? baseOf(undefined, ctx.config.url);
      const ide = encodeURIComponent(ctx.brand.NAME);
      const nonce = ctx.crypto.random({ bytes: 16, output: "base64url" });
      return { url: `${base}${AUTH}?ideName=${ide}&port=${callback}`, state: nonce, verifier: "", redirect_uri: "", carried: { base } };
    },
    answer({ request }: { request: Request }, ctx: Ctx) {
      if (request.method === "GET" && request.path === STATUS) {
        const status = { ideName: ctx.brand.NAME, description: "", needsToken: true, capabilities: { canOpenFixSuggestion: false } };
        return { status: 200, body: status };
      }
      if (request.method !== "POST" || request.path !== TOKEN) return { status: 404, body: null };
      const token = (request.body as { token?: unknown } | null)?.token;
      if (typeof token !== "string" || token.trim() === "") return { status: 400, body: null };
      const grant = { access_token: token.trim(), refresh_token: null, expires_at: null, scope: null, client: null };
      return { status: 200, body: "OK", grant };
    },
    refresh: ({ grant }: { grant: unknown }) => grant,
  };
}
