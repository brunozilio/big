import { ActionError, DEFAULT_RETRY_AFTER_MS, form, regional, send } from "sdk/connectors/index.ts";

const LIFETIME = 3600;

export type Held = { token: string; tenant_url: string; account: string };

type Client = { region: string; client_id: string; client_secret: string; tenant_url: string; account: string };

const randomToken = (ctx) => ctx.crypto.random({ bytes: 32, output: "base64url" });

const challenge = (ctx, verifier: string) => ctx.crypto.sha256({ data: verifier, output: "base64url" });

const oidc = (ctx, region: string) => regional(ctx.config.oidc_url, region);

const port = (ctx) => Number.parseInt(ctx.config.loopback_port, 10);

const redirect = (ctx) => ctx.config.redirect.replace("{port}", String(port(ctx)));

const regionName = (value: string) => value.length <= 32 && /^[a-z]{2}(-[a-z]+)+-\d+$/.test(value);

function isoSeconds(second: number): string {
  return new Date(second * 1000).toISOString().replace(".000Z", "Z");
}

export function heldBy(token: string): Held | null {
  try {
    const found = JSON.parse(token);
    return typeof found?.token === "string" && typeof found?.tenant_url === "string" ? found : null;
  } catch (_) {
    return null;
  }
}

function parsed(body: string) {
  try {
    return JSON.parse(body);
  } catch (_) {
    return null;
  }
}

function refused(status: number, answer, what: string): never {
  const told = `${what} answered ${status}: ${String(answer?.error ?? "")} ${String(answer?.error_description ?? answer?.message ?? "")}`.trimEnd();
  if (status >= 500) throw new ActionError("transient", told);
  if (status === 429) throw new ActionError("rate_limited", told, DEFAULT_RETRY_AFTER_MS);
  throw new ActionError("auth", told);
}

function posted(ctx, url: string, body: unknown, what: string) {
  const answer = send(ctx, { method: "POST", url, headers: { "content-type": "application/json", accept: "application/json", "user-agent": ctx.config.oidc_user_agent }, body: JSON.stringify(body) });
  const found = parsed(answer.body);
  if (answer.status < 200 || answer.status >= 300) refused(answer.status, found, what);
  return found;
}

export function discovery(ctx, nonce: string): string {
  const surface = ctx.config.surface.replace("{nonce}", nonce).replace("{port}", String(port(ctx))).replace("{version}", ctx.config.app_version);
  const start = ctx.config.discovery === "enterprise" ? ctx.config.enterprise_url : ctx.config.social_url;
  return start.replace("{surface}", encodeURIComponent(surface));
}

function discovered(query: Record<string, string>): Client {
  const value = (name: string) => (typeof query?.[name] === "string" ? query[name].trim() : "");
  const region = value("idcRegion");
  if (!regionName(region)) throw new ActionError("auth", `Amazon Quick did not name the account's AWS region (idcRegion \`${region}\`)`);
  for (const name of ["issuerUrl", "tenantUrl"]) {
    if (!value(name).startsWith("https://")) throw new ActionError("auth", `Amazon Quick sent no https ${name}`);
  }
  return { region, client_id: "", client_secret: "", tenant_url: value("tenantUrl").replace(/\/+$/, ""), account: value("account") };
}

function registered(ctx, client: Client, issuer: string): Client {
  const body = {
    clientName: ctx.config.client_name,
    clientType: "public",
    scopes: ctx.config.scopes.split(/\s+/).filter((scope: string) => scope !== ""),
    grantTypes: ["authorization_code", "refresh_token"],
    redirectUris: [redirect(ctx)],
    issuerUrl: issuer,
  };
  const found = posted(ctx, `${oidc(ctx, client.region)}/client/register`, body, "the identity center");
  if (typeof found?.clientId !== "string" || typeof found?.clientSecret !== "string") throw new ActionError("auth", "the identity center registered no client");
  return { ...client, client_id: found.clientId, client_secret: found.clientSecret };
}

function grantOf(client: Client, found, previous: string | null, now: number) {
  if (typeof found?.accessToken !== "string") throw new ActionError("auth", "the identity center gave no access token");
  const refresh = typeof found.refreshToken === "string" ? found.refreshToken : previous;
  const expiresAt = isoSeconds(now + (Number.isInteger(found.expiresIn) && found.expiresIn > 0 ? found.expiresIn : LIFETIME));
  const held: Held = { token: found.accessToken, tenant_url: client.tenant_url, account: client.account };
  return { access_token: JSON.stringify(held), refresh_token: refresh, expires_at: expiresAt, scope: null, client: null, extra: client };
}

function clientOf(value): Client {
  const fields = ["region", "client_id", "client_secret", "tenant_url"];
  if (!value || !fields.every((name) => typeof value[name] === "string" && value[name] !== "")) {
    throw new ActionError("auth", "sign in to Amazon Quick again");
  }
  return value as Client;
}

export const signIn = {
  name: (_, ctx) => ctx.config.client_name,
  port: (_, ctx) => port(ctx),
  begin(_, ctx) {
    const nonce = randomToken(ctx);
    return { url: discovery(ctx, nonce), state: nonce, verifier: "", redirect_uri: redirect(ctx), carried: null };
  },
  proceed({ authorization, query }, ctx) {
    if (query?.nonce !== authorization.state) throw new ActionError("auth", "the answer does not belong to this sign-in");
    const client = registered(ctx, discovered(query), query.issuerUrl);
    const state = randomToken(ctx);
    const verifier = randomToken(ctx);
    const pairs: [string, string][] = [
      ["response_type", "code"],
      ["client_id", client.client_id],
      ["redirect_uri", redirect(ctx)],
      ["scopes", ctx.config.scopes],
      ["state", state],
      ["code_challenge", challenge(ctx, verifier)],
      ["code_challenge_method", "S256"],
    ];
    const url = `${oidc(ctx, client.region)}/authorize?${form(pairs)}`;
    return { authorization: { url, state, verifier, redirect_uri: redirect(ctx), carried: client } };
  },
  exchange({ authorization, code, now }, ctx) {
    const client = clientOf(authorization.carried);
    const body = { clientId: client.client_id, clientSecret: client.client_secret, grantType: "authorization_code", code, redirectUri: authorization.redirect_uri, codeVerifier: authorization.verifier };
    return grantOf(client, posted(ctx, `${oidc(ctx, client.region)}/token`, body, "the identity center"), null, now);
  },
  refresh({ grant, now }, ctx) {
    const client = clientOf(grant?.extra);
    if (typeof grant.refresh_token !== "string") throw new ActionError("auth", "there is no refresh token; sign in to Amazon Quick again");
    const body = { clientId: client.client_id, clientSecret: client.client_secret, grantType: "refresh_token", refreshToken: grant.refresh_token };
    return { ...grantOf(client, posted(ctx, `${oidc(ctx, client.region)}/token`, body, "the identity center"), grant.refresh_token, now), method: grant.method ?? null };
  },
};
