import { type Tokens, form, jsonRequest, later, parsed, text, unauthorized } from "sdk/providers/index.ts";

const LIFETIME = 3600;

export function portal(ctx, challenge: string, state: string, redirect: string): string {
  const path = ctx.config.callback_path;
  const origin = redirect.endsWith(path) ? redirect.slice(0, -path.length) : redirect;
  const query = form(ctx, [
    ["state", state],
    ["code_challenge", challenge],
    ["code_challenge_method", "S256"],
    ["redirect_uri", origin],
    ["redirect_from", "kirocli"],
  ]);
  return `${ctx.config.portal_url}?${query}`;
}

export function redirect(ctx, callback: string, query: string): string {
  const option = ctx.oauth.query({ query }).login_option ?? "";
  if (option === "google" || option === "github") return `${callback}?${form(ctx, [["login_option", option]])}`;
  if (option === "") unauthorized("Kiro's sign-in did not say how you signed in");
  return unauthorized(`signing in to Kiro with ${option} is not supported yet; sign in with Google or GitHub`);
}

export const exchange = (ctx, code: string, verifier: string, redirectUri: string) =>
  jsonRequest(ctx.config.token_url, { code, code_verifier: verifier, redirect_uri: redirectUri });

export const refresh = (ctx, previous: Tokens) => jsonRequest(ctx.config.refresh_url, { refreshToken: previous.refresh_token ?? "" });

export function tokens(ctx, { status, body, previous, now }): Tokens {
  if (status !== 200) unauthorized(`Kiro's auth service returned HTTP ${status}`);
  const value = parsed(body) ?? unauthorized("invalid Kiro token response");
  const access_token = text(value.accessToken) ?? unauthorized("Kiro returned no access token");
  const refresh_token = text(value.refreshToken) ?? text(previous?.refresh_token) ?? unauthorized("Kiro returned no refresh token");
  const account_id = text(value.profileArn) ?? text(previous?.account_id) ?? unauthorized("Kiro returned no profile ARN");
  const lifetime = Number.isInteger(value.expiresIn) && value.expiresIn > 0 ? value.expiresIn : LIFETIME;
  return { access_token, refresh_token, account_id, expires_at: later(ctx, now, lifetime), email: null, subject: null };
}
