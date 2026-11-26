import { type Tokens, form, jsonRequest, later, parsed, regional, text, unauthorized, words } from "sdk/providers/index.ts";

const LIFETIME = 3600;

export type Client = { region: string; client_id: string; client_secret: string };

const regionName = (text: string) => text !== "" && text.length <= 32 && text.includes("-") && /^[a-z0-9-]+$/.test(text);

export function portalAnswer(ctx, path: string, query: string): { social: true } | { start_url: string; region: string } {
  const given = ctx.oauth.query({ query });
  const value = (name: string) => (typeof given[name] === "string" && given[name].trim() !== "" ? given[name].trim() : undefined);
  const option = (value("login_option") ?? "").toLowerCase();
  if ((option === "google" || option === "github") && path === ctx.config.callback_path) return { social: true };
  if (option === "awsidc" || option === "builderid") {
    const builder = option === "builderid";
    const start_url = value("issuer_url") ?? (builder ? ctx.config.builder_id_url : undefined);
    const region = value("idc_region") ?? value("sso_region") ?? (builder ? "us-east-1" : undefined);
    if (start_url === undefined) {
      unauthorized(
        "Kiro's portal did not say which organization to sign in to (no issuer_url); check the start URL on the \"Your organization\" option and try again",
      );
    }
    if (!start_url.startsWith("https://")) unauthorized(`the organization's start URL is not https: ${start_url}`);
    if (region === undefined) unauthorized("Kiro's portal did not say the organization's AWS region (no idc_region)");
    if (!regionName(region)) unauthorized(`\`${region}\` is not an AWS region`);
    return { start_url, region };
  }
  if (option === "") unauthorized("Kiro's sign-in did not say how you signed in");
  return unauthorized(`signing in to Kiro with ${option} is not supported yet`);
}

const oidc = (ctx, region: string) => regional(ctx.config.oidc_url, region);

export function register(ctx, region: string, startUrl: string, redirect: string) {
  return jsonRequest(`${oidc(ctx, region)}/client/register`, {
    clientName: ctx.config.client_name,
    clientType: "public",
    scopes: words(ctx.config.scopes),
    grantTypes: ["authorization_code", "refresh_token"],
    redirectUris: [redirect],
    issuerUrl: startUrl,
  });
}

export function registered(region: string, status: number, body: string): Client {
  if (status !== 200) unauthorized(`the organization's sign-in service returned HTTP ${status}`);
  const value = parsed(body) ?? unauthorized("invalid client registration");
  const client_id = text(value.clientId);
  const client_secret = text(value.clientSecret);
  if (client_id === undefined || client_secret === undefined) unauthorized("the organization's sign-in service registered no client");
  return { region, client_id, client_secret };
}

export function authorize(ctx, client: Client, redirect: string, state: string, challenge: string): string {
  const query = form(ctx, [
    ["response_type", "code"],
    ["client_id", client.client_id],
    ["redirect_uri", redirect],
    ["scopes", ctx.config.scopes],
    ["state", state],
    ["code_challenge", challenge],
    ["code_challenge_method", "S256"],
  ]);
  return `${oidc(ctx, client.region)}/authorize?${query}`;
}

export function exchange(ctx, client: Client, code: string, verifier: string, redirect: string) {
  return jsonRequest(`${oidc(ctx, client.region)}/token`, {
    clientId: client.client_id,
    clientSecret: client.client_secret,
    grantType: "authorization_code",
    code,
    redirectUri: redirect,
    codeVerifier: verifier,
  });
}

export function refresh(ctx, client: Client, previous: Tokens) {
  return jsonRequest(`${oidc(ctx, client.region)}/token`, {
    clientId: client.client_id,
    clientSecret: client.client_secret,
    grantType: "refresh_token",
    refreshToken: previous.refresh_token ?? "",
  });
}

export function tokens(ctx, client: Client, { status, body, previous, now }): Tokens {
  if (status !== 200) unauthorized(`the organization's sign-in service returned HTTP ${status}`);
  const value = parsed(body) ?? unauthorized("invalid token response");
  const access_token = text(value.accessToken) ?? unauthorized("the sign-in returned no access token");
  const refresh_token = text(value.refreshToken) ?? text(previous?.refresh_token) ?? unauthorized("the sign-in returned no refresh token");
  const lifetime = Number.isInteger(value.expiresIn) && value.expiresIn > 0 ? value.expiresIn : LIFETIME;
  return { access_token, refresh_token, account_id: previous?.account_id ?? "", expires_at: later(ctx, now, lifetime), client, email: null, subject: null };
}

export function clientOf(tokens: Tokens): Client | undefined {
  const client = tokens?.client as Client | undefined;
  return client && typeof client.region === "string" && typeof client.client_id === "string" && typeof client.client_secret === "string" ? client : undefined;
}

export function firstProfile(status: number, body: string): string | undefined {
  if (status !== 200) return undefined;
  const profiles = parsed(body)?.profiles;
  return Array.isArray(profiles) ? profiles.map((profile) => profile?.arn).find((arn) => typeof arn === "string") : undefined;
}
