import { ActionError, RECONNECT, checked, decode, type Language, pick, send } from "sdk/connectors/index.ts";
import { isoSeconds } from "./login.ts";

export const SCOPE = "sso:account:access";
export const DEVICE_GRANT = "urn:ietf:params:oauth:grant-type:device_code";
const DEFAULT_INTERVAL = 5;
const DEFAULT_EXPIRES_IN = 900;

export const oidc = (region: string) => `https://oidc.${region}.amazonaws.com`;

function text(answer, names: string[]): string | undefined {
  for (const name of names) if (typeof answer?.[name] === "string") return answer[name];
  return undefined;
}

function number(answer, names: string[]): number | undefined {
  for (const name of names) if (Number.isInteger(answer?.[name])) return answer[name];
  return undefined;
}

function parsed(body: string) {
  try {
    return JSON.parse(body);
  } catch (_) {
    return null;
  }
}

function errorCode(answer): string | undefined {
  const code = text(answer, ["error", "__type"]);
  return code === undefined ? undefined : (code.split("#").pop() ?? "").toLowerCase().replace(/_/g, "");
}

function refusedWith(code: string, answer): never {
  const description = text(answer, ["error_description", "message", "Message"]);
  throw new ActionError("auth", description === undefined ? code : `${code}: ${description}`);
}

function post(ctx, url: string, body: Record<string, unknown>) {
  const response = send(ctx, {
    method: "POST",
    url,
    headers: { accept: "application/json", "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return { response, answer: parsed(response.body) };
}

function answered(ctx, url: string, body: Record<string, unknown>) {
  const { response, answer } = post(ctx, url, body);
  const code = errorCode(answer);
  if (code !== undefined) refusedWith(code, answer);
  checked(response);
  return answer;
}

function granted(answer, now: number) {
  const access = text(answer, ["access_token", "accessToken"]);
  if (access === undefined) throw new ActionError("auth", "the answer has no access token");
  const expiresIn = number(answer, ["expires_in", "expiresIn"]);
  return {
    access_token: access,
    refresh_token: text(answer, ["refresh_token", "refreshToken"]) ?? null,
    expires_at: expiresIn === undefined ? null : isoSeconds(now + expiresIn),
    scope: text(answer, ["scope"]) ?? null,
    client: null,
    extra: null,
  };
}

const regionIn = (ctx, entered: unknown) =>
  typeof entered === "string" && entered.trim() !== "" ? entered.trim() : ctx.config.region;

function register(ctx, region: string) {
  const registration = {
    clientName: ctx.brand.NAME,
    clientType: "public",
    scopes: [SCOPE],
    grantTypes: [DEVICE_GRANT, "refresh_token"],
  };
  const answer = answered(ctx, `${oidc(region)}/client/register`, registration);
  const needed = (name: string) => text(answer, [name]) ?? decode(`the registration has no ${name}`);
  return { client_id: needed("clientId"), client_secret: needed("clientSecret") };
}

function deviceCode(answer) {
  const needed = (names: string[]) => text(answer, names) ?? decode(`the device answer has no ${names[0]}`);
  return {
    device_code: needed(["device_code", "deviceCode"]),
    user_code: needed(["user_code", "userCode"]),
    verification_uri: needed(["verification_uri", "verificationUri", "verification_url"]),
    verification_uri_complete: text(answer, ["verification_uri_complete", "verificationUriComplete"]) ?? null,
    expires_in: number(answer, ["expires_in", "expiresIn"]) ?? DEFAULT_EXPIRES_IN,
    interval: Math.max(1, number(answer, ["interval"]) ?? DEFAULT_INTERVAL),
  };
}

const POLLED: Record<string, Record<string, string>> = {
  authorizationpending: { state: "pending" },
  authorizationpendingexception: { state: "pending" },
  slowdown: { state: "slow_down" },
  slowdownexception: { state: "slow_down" },
  expiredtoken: { state: "expired" },
  expiredtokenexception: { state: "expired" },
  codeexpired: { state: "expired" },
  accessdenied: { state: "denied" },
  accessdeniedexception: { state: "denied" },
  authorizationdeclined: { state: "denied" },
};

export const sso = {
  name: () => "aws-sso",
  fields: ({ language }: { language: Language }, ctx) => [
    {
      name: "start_url",
      label: pick(language, "IAM Identity Center Start URL", "Start URL de IAM Identity Center", "Start URL do IAM Identity Center"),
      placeholder: pick(language, "https://company.awsapps.com/start", "https://empresa.awsapps.com/start", "https://empresa.awsapps.com/start"),
    },
    { name: "region", label: pick(language, "Region", "Región", "Região"), value: ctx.config.region },
  ],
  start({ fields, language }, ctx) {
    const startUrl = typeof fields?.start_url === "string" ? fields.start_url.trim() : "";
    if (startUrl === "") {
      throw new Error(
        pick(language, "Enter the IAM Identity Center Start URL.", "Indica la Start URL de IAM Identity Center.", "Informe a Start URL do IAM Identity Center."),
      );
    }
    const region = regionIn(ctx, fields?.region);
    const client = register(ctx, region);
    const body = { clientId: client.client_id, clientSecret: client.client_secret, startUrl };
    const code = deviceCode(answered(ctx, `${oidc(region)}/device_authorization`, body));
    return { code, carried: { client, region } };
  },
  poll({ started, now }, ctx) {
    const { client, region } = started.carried;
    const body = { clientId: client.client_id, clientSecret: client.client_secret, grantType: DEVICE_GRANT, deviceCode: started.code.device_code };
    const { response, answer } = post(ctx, `${oidc(region)}/token`, body);
    const code = errorCode(answer);
    if (code !== undefined) {
      const known = POLLED[code];
      if (known === undefined) refusedWith(code, answer);
      return known.state === "slow_down" ? { state: "slow_down", interval: number(answer, ["interval"]) ?? null } : known;
    }
    checked(response);
    return { state: "authorized", grant: { ...granted(answer, now), client, extra: { region } } };
  },
  refresh({ grant, now }, ctx) {
    const client = grant?.client;
    if (!client) throw new ActionError("auth", `${RECONNECT}: the registered client is missing`);
    const refresh = grant.refresh_token;
    if (typeof refresh !== "string") throw new ActionError("auth", "there is no refresh token");
    const region = regionIn(ctx, grant?.extra?.region);
    const body = { clientId: client.client_id, clientSecret: client.client_secret, grantType: "refresh_token", refreshToken: refresh };
    const fresh = granted(answered(ctx, `${oidc(region)}/token`, body), now);
    return { ...fresh, refresh_token: fresh.refresh_token ?? refresh, client, extra: grant.extra ?? { region }, method: grant.method ?? null };
  },
};
