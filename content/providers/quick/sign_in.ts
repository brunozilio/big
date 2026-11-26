import { type Tokens, described, form, jsonRequest, later, parsed, post, regional, text, unauthorized, words } from "sdk/providers/index.ts";

const LIFETIME = 3600;

export type Client = { region: string; client_id: string; client_secret: string; tenant_url: string; issuer_url: string };

type Found = { account: string; issuer_url: string; tenant_url: string; region: string };

const regionName = (value: string) => value.length <= 32 && /^[a-z]{2}(-[a-z]+)+-\d+$/.test(value);

const secure = (value: string, what: string) => (value.startsWith("https://") ? value : unauthorized(`${what} is not https: ${value}`));

export function discovered(ctx, query: string): Found {
  const given = ctx.oauth.query({ query });
  const value = (name: string) => (typeof given[name] === "string" ? given[name].trim() : "");
  const region = value("idcRegion");
  if (!regionName(region)) unauthorized(`the sign-in did not name an AWS region (idcRegion \`${region}\`)`);
  return {
    account: value("account"),
    issuer_url: secure(value("issuerUrl"), "the identity center"),
    tenant_url: secure(value("tenantUrl"), "the account's address"),
    region,
  };
}

export const oidc = (ctx, region: string) => regional(ctx.config.oidc_url, region);

export function surface(ctx, nonce: string, port: number): string {
  return ctx.config.surface.replace("{nonce}", nonce).replace("{port}", String(port)).replace("{version}", ctx.config.app_version);
}

export function discovery(ctx, nonce: string, port: number): string {
  const start = ctx.config.discovery === "enterprise" ? ctx.config.enterprise_url : ctx.config.social_url;
  return start.replace("{surface}", encodeURIComponent(surface(ctx, nonce, port)));
}

export function register(ctx, found: Found, redirect: string) {
  return jsonRequest(`${oidc(ctx, found.region)}/client/register`, {
    clientName: ctx.config.client_name,
    clientType: "public",
    scopes: words(ctx.config.scopes),
    grantTypes: ["authorization_code", "refresh_token"],
    redirectUris: [redirect],
    issuerUrl: found.issuer_url,
  });
}

export function registered(found: Found, status: number, body: string): Client {
  if (status !== 200) unauthorized(`the identity center refused to register the sign-in (HTTP ${status})`);
  const value = parsed(body) ?? unauthorized("the identity center answered the registration with something other than JSON");
  const client_id = text(value.clientId) ?? unauthorized("the identity center registered no client id");
  const client_secret = text(value.clientSecret) ?? unauthorized("the identity center registered no client secret");
  return { region: found.region, client_id, client_secret, tenant_url: found.tenant_url, issuer_url: found.issuer_url };
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

export function clientOf(tokens: Tokens | null | undefined): Client | undefined {
  const client = tokens?.client as Client | undefined;
  const fields = ["region", "client_id", "client_secret", "tenant_url"];
  return client && fields.every((name) => typeof client[name] === "string" && client[name] !== "") ? client : undefined;
}

export function tokens(ctx, client: Client, account: string, { status, body, previous, now }): Tokens {
  if (status !== 200) unauthorized(`the identity center refused the sign-in (HTTP ${status})`);
  const value = parsed(body) ?? unauthorized("the identity center answered with something other than JSON");
  const access_token = text(value.accessToken) ?? unauthorized("the sign-in returned no access token");
  const refresh_token = text(value.refreshToken) ?? text(previous?.refresh_token) ?? unauthorized("the sign-in returned no refresh token");
  const lifetime = Number.isInteger(value.expiresIn) && value.expiresIn > 0 ? value.expiresIn : LIFETIME;
  return { access_token, refresh_token, account_id: account, expires_at: later(ctx, now, lifetime), client, email: null, subject: null };
}

function settled<T>(ctx, answer: number, work: () => T): T {
  try {
    return work();
  } catch (error) {
    ctx.callback.finish({ answer, error: described(error) });
    throw error;
  }
}

export function signIn(ctx): Tokens {
  const listened = ctx.callback.listen({ port: Number.parseInt(ctx.config.callback_port, 10) || 0, path: ctx.config.callback_path });
  const nonce = ctx.crypto.random(32);
  ctx.browser.open({ url: discovery(ctx, nonce, listened.port), code: null });
  const arrived = ctx.callback.next({ listener: listened.listener, key: "nonce", state: nonce, others: [] });
  const found = settled(ctx, arrived.answer, () => discovered(ctx, arrived.query));
  const redirect = ctx.config.redirect.replace("{port}", String(listened.port));
  const client = settled(ctx, arrived.answer, () => {
    const { status, body } = post(ctx, register(ctx, found, redirect));
    return registered(found, status, body);
  });
  const verifier = ctx.crypto.random(64);
  const state = ctx.crypto.random(32);
  ctx.callback.redirect({ answer: arrived.answer, to: authorize(ctx, client, redirect, state, ctx.oauth.challenge(verifier)) });
  const back = ctx.callback.next({ listener: listened.listener, state, others: ["/"] }).answer;
  return settled(ctx, back, () => {
    const code = ctx.callback.code({ answer: back });
    const { status, body } = post(ctx, exchange(ctx, client, code, verifier, redirect));
    const signed = tokens(ctx, client, found.account, { status, body, previous: null, now: ctx.clock.now() });
    ctx.callback.signedIn({ answer: back, account: found.account === "" ? null : found.account });
    return signed;
  });
}
