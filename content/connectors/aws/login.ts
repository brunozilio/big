import { ActionError, DEFAULT_RETRY_AFTER_MS, decode, form, send } from "sdk/connectors/index.ts";
import { keysOf } from "./sts.ts";

export const REDIRECT = "http://127.0.0.1:{port}/oauth/callback";

export const base = (region: string) => `https://${region}.signin.aws.amazon.com`;

export const clientId = (ctx) => ctx.config.login_client_id;

export function isoSeconds(second: number): string {
  return new Date(second * 1000).toISOString().replace(".000Z", "Z");
}

const base64url = (ctx, text: string) => ctx.encoding.convert({ data: text, from: "text", to: "base64url" });

export const randomToken = (ctx) => ctx.crypto.random({ bytes: 32, output: "base64url" });

export const challenge = (ctx, verifier: string) => ctx.crypto.sha256({ data: verifier, output: "base64url" });

export function proof(ctx, key: string, uri: string, now: number): string {
  const jwk = ctx.crypto.ecdsa.public({ private: key });
  const header = { typ: "dpop+jwt", alg: "ES256", jwk: { kty: "EC", x: jwk.x, y: jwk.y, crv: "P-256" } };
  const payload = { htm: "POST", htu: uri, iat: now, jti: randomToken(ctx) };
  const input = `${base64url(ctx, JSON.stringify(header))}.${base64url(ctx, JSON.stringify(payload))}`;
  return `${input}.${ctx.crypto.ecdsa.sign({ private: key, data: input, output: "base64url" })}`;
}

function accountOf(ctx, idToken: string): string {
  const part = idToken.split(".")[1];
  if (!part) return "";
  try {
    const claims = JSON.parse(ctx.encoding.convert({ data: part, from: "base64url", to: "text" }));
    return String(claims?.sub ?? "").split(":")[4] ?? "";
  } catch (_) {
    return "";
  }
}

function refused(status: number, answer): never {
  const code = typeof answer?.error === "string" ? answer.error : "";
  const message = typeof answer?.message === "string" ? answer.message : typeof answer?.Message === "string" ? answer.Message : "";
  const told = `the sign-in answered ${status}: ${code} ${message}`.trimEnd();
  if (status >= 500) throw new ActionError("transient", told);
  if (status === 429) throw new ActionError("rate_limited", told, DEFAULT_RETRY_AFTER_MS);
  throw new ActionError("auth", told);
}

function parsed(body: string) {
  try {
    return JSON.parse(body);
  } catch (_) {
    return null;
  }
}

function token(ctx, region: string, key: string, body: Record<string, string>, now: number) {
  const url = `${base(region)}/v1/token`;
  const headers = { dpop: proof(ctx, key, url, now), accept: "application/json", "content-type": "application/json" };
  const response = send(ctx, { method: "POST", url, headers, body: JSON.stringify(body) });
  const answer = parsed(response.body);
  if (response.status < 200 || response.status >= 300) refused(response.status, answer);
  const output = answer?.tokenOutput ?? answer;
  if (typeof output?.accessToken?.accessKeyId !== "string") return decode("the sign-in gave no credentials");
  return output;
}

function grantOf(ctx, region: string, output, key: string, account: string, now: number) {
  const expiresAt = isoSeconds(now + (Number.isInteger(output.expiresIn) ? output.expiresIn : 900));
  const held = output.accessToken;
  const keys = {
    accessKeyId: held.accessKeyId ?? "",
    secretAccessKey: held.secretAccessKey ?? "",
    sessionToken: held.sessionToken ?? "",
    accountId: account,
    expiresAt,
  };
  return {
    access_token: JSON.stringify(keys),
    refresh_token: typeof output.refreshToken === "string" ? output.refreshToken : null,
    expires_at: expiresAt,
    scope: null,
    client: null,
    extra: { proof_key: key, region, client_id: clientId(ctx) },
  };
}

const regionOf = (ctx) => ctx.config.region;

export const signIn = {
  name: (_, ctx) => clientId(ctx),
  begin({ port }, ctx) {
    const state = randomToken(ctx);
    const verifier = randomToken(ctx);
    const redirect_uri = REDIRECT.replace("{port}", String(port));
    const query = form([
      ["response_type", "code"],
      ["client_id", clientId(ctx)],
      ["state", state],
      ["code_challenge_method", "SHA-256"],
      ["scope", "openid"],
      ["redirect_uri", redirect_uri],
      ["code_challenge", challenge(ctx, verifier)],
    ]);
    return { url: `${base(regionOf(ctx))}/v1/authorize?${query}`, state, verifier, redirect_uri, carried: null };
  },
  exchange({ authorization, code, now }, ctx) {
    const key = ctx.crypto.ecdsa.generate().private;
    const body = {
      clientId: clientId(ctx),
      grantType: "authorization_code",
      code,
      codeVerifier: authorization.verifier,
      redirectUri: authorization.redirect_uri,
    };
    const output = token(ctx, regionOf(ctx), key, body, now);
    return grantOf(ctx, regionOf(ctx), output, key, accountOf(ctx, output.idToken ?? ""), now);
  },
  refresh({ grant, now }, ctx) {
    const key = grant?.extra?.proof_key;
    if (typeof key !== "string" || key === "") throw new ActionError("auth", "the proof key is missing");
    const refresh = grant.refresh_token;
    if (typeof refresh !== "string") throw new ActionError("auth", "there is no refresh token");
    const region = typeof grant.extra.region === "string" ? grant.extra.region : regionOf(ctx);
    const body = { clientId: clientId(ctx), grantType: "refresh_token", refreshToken: refresh };
    const output = token(ctx, region, key, body, now);
    const account = keysOf(grant.access_token)?.accountId ?? "";
    return { ...grantOf(ctx, region, output, key, account, now), method: grant.method ?? null };
  },
};
