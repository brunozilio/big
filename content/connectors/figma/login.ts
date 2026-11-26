import { ActionError, checked, form, json, send } from "sdk/connectors/index.ts";

export const SCOPES = "file_content:read current_user:read";

const REDIRECT = "http://127.0.0.1:{port}/oauth/callback";

type Config = { client_id?: string; client_secret?: string; oauth_url?: string; api_url: string; callback_port?: string | number };

export const configured = (config: Config) => String(config.client_id ?? "").trim() !== "";

const randomToken = (ctx) => ctx.crypto.random({ bytes: 32, output: "base64url" });

function isoSeconds(second: number): string {
  return new Date(second * 1000).toISOString().replace(".000Z", "Z");
}

function basic(ctx): string {
  const pair = `${ctx.config.client_id}:${ctx.config.client_secret ?? ""}`;
  return `Basic ${ctx.encoding.convert({ data: pair, from: "text", to: "base64" })}`;
}

function post(ctx, path: string, fields: [string, string][]) {
  const url = `${String(ctx.config.api_url).replace(/\/+$/, "")}${path}`;
  const headers = { authorization: basic(ctx), "content-type": "application/x-www-form-urlencoded", accept: "application/json" };
  return json(checked(send(ctx, { method: "POST", url, headers, body: form(fields) })));
}

function grantOf(answer, now: number, refresh: string | null) {
  if (typeof answer?.access_token !== "string" || answer.access_token === "") throw new ActionError("auth", "Figma gave no access token");
  const seconds = Number.isFinite(Number(answer.expires_in)) ? Number(answer.expires_in) : 3600;
  const kept = typeof answer.refresh_token === "string" && answer.refresh_token !== "" ? answer.refresh_token : refresh;
  return { access_token: answer.access_token, refresh_token: kept, expires_at: isoSeconds(now + seconds), scope: SCOPES, client: null, method: "oauth" };
}

export const signIn = {
  name: (_, ctx) => String(ctx.config.client_id ?? ""),
  port: (_, ctx) => {
    const port = Number(ctx.config.callback_port ?? 0);
    return Number.isInteger(port) && port > 0 ? port : null;
  },
  begin({ port }, ctx) {
    const state = randomToken(ctx);
    const verifier = randomToken(ctx);
    const redirect_uri = REDIRECT.replace("{port}", String(port));
    const query = form([
      ["client_id", String(ctx.config.client_id)],
      ["redirect_uri", redirect_uri],
      ["scope", SCOPES],
      ["state", state],
      ["response_type", "code"],
      ["code_challenge", ctx.crypto.sha256({ data: verifier, output: "base64url" })],
      ["code_challenge_method", "S256"],
    ]);
    return { url: `${ctx.config.oauth_url}?${query}`, state, verifier, redirect_uri, carried: null };
  },
  exchange({ authorization, code, now }, ctx) {
    const answer = post(ctx, "/v1/oauth/token", [
      ["redirect_uri", authorization.redirect_uri],
      ["code", code],
      ["grant_type", "authorization_code"],
      ["code_verifier", authorization.verifier],
    ]);
    return grantOf(answer, now, null);
  },
  refresh({ grant, now }, ctx) {
    const refresh = grant?.refresh_token;
    if (typeof refresh !== "string" || refresh === "") throw new ActionError("auth", "there is no refresh token; sign in to Figma again");
    return grantOf(post(ctx, "/v1/oauth/refresh", [["refresh_token", refresh]]), now, refresh);
  },
};
