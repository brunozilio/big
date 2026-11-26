import { decode, fetch, regional, type Request } from "sdk/connectors/index.ts";
import { sign, stampOf } from "./sigv4.ts";

const BODY = "Action=GetCallerIdentity&Version=2011-06-15";
const FORM = "application/x-www-form-urlencoded; charset=utf-8";

export type Keys = {
  accessKeyId: string;
  secretAccessKey: string;
  sessionToken: string;
  accountId: string;
  expiresAt: string;
};

export type Caller = { account: string; arn: string; userId: string };

export function keysOf(accessToken: string): Keys | undefined {
  let read;
  try {
    read = JSON.parse(accessToken);
  } catch (_) {
    return undefined;
  }
  if (!read || typeof read !== "object" || typeof read.accessKeyId !== "string" || read.accessKeyId === "") return undefined;
  if (typeof read.secretAccessKey !== "string") return undefined;
  return {
    accessKeyId: read.accessKeyId,
    secretAccessKey: read.secretAccessKey,
    sessionToken: typeof read.sessionToken === "string" ? read.sessionToken : "",
    accountId: typeof read.accountId === "string" ? read.accountId : "",
    expiresAt: typeof read.expiresAt === "string" ? read.expiresAt : "",
  };
}

export function signedIdentityRequest(ctx, keys: Keys, region: string, now: Date): Request {
  const url = regional(ctx.config.sts_url, region);
  const headers: Record<string, string> = { "content-type": FORM };
  const added = sign(
    ctx,
    { method: "POST", url, headers: [["content-type", FORM]], body: BODY, region, service: "sts", stamp: stampOf(now) },
    keys,
  );
  for (const [name, value] of added) headers[name] = value;
  return { method: "POST", url, headers, body: BODY };
}

export function element(xml: string, name: string): string {
  const open = `<${name}>`;
  const close = `</${name}>`;
  const start = xml.indexOf(open);
  if (start < 0) return "";
  const rest = xml.slice(start + open.length);
  const end = rest.indexOf(close);
  return end < 0 ? "" : rest.slice(0, end).trim();
}

export function caller(ctx, keys: Keys, region: string): Caller {
  const answer = fetch(ctx, signedIdentityRequest(ctx, keys, region, new Date())).body;
  const found = { account: element(answer, "Account"), arn: element(answer, "Arn"), userId: element(answer, "UserId") };
  if (found.account === "") return decode("the identity has no account");
  return found;
}
