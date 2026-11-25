export type Keys = { accessKeyId: string; secretAccessKey: string; sessionToken?: string };

export type Signing = {
  method: string;
  url: string;
  headers: [string, string][];
  body: string;
  region: string;
  service: string;
  stamp: string;
};

const ALGORITHM = "AWS4-HMAC-SHA256";

const two = (value: number) => String(value).padStart(2, "0");

export function stampOf(date: Date): string {
  return `${date.getUTCFullYear()}${two(date.getUTCMonth() + 1)}${two(date.getUTCDate())}T${two(date.getUTCHours())}${two(date.getUTCMinutes())}${two(date.getUTCSeconds())}Z`;
}

const sha256 = (ctx, data: string) => ctx.crypto.sha256({ data, output: "hex" });

const hmac = (ctx, key: string, keyEncoding: string, data: string) => ctx.crypto.hmac({ key, keyEncoding, data, output: "hex" });

export function signature(ctx, secret: string, stamp: string, region: string, service: string, canonical: string): string {
  const day = stamp.slice(0, 8);
  const scope = `${day}/${region}/${service}/aws4_request`;
  const toSign = `${ALGORITHM}\n${stamp}\n${scope}\n${sha256(ctx, canonical)}`;
  let key = hmac(ctx, `AWS4${secret}`, "text", day);
  for (const part of [region, service, "aws4_request"]) key = hmac(ctx, key, "hex", part);
  return hmac(ctx, key, "hex", toSign);
}

const encode = (text: string) =>
  encodeURIComponent(text).replace(/[!'()*]/g, (character) => `%${character.charCodeAt(0).toString(16).toUpperCase()}`);

function decoded(text: string): string {
  return text.replace(/(%[0-9a-fA-F]{2})+/g, (run) => {
    try {
      return decodeURIComponent(run);
    } catch (_) {
      return run;
    }
  });
}

type Parts = { scheme: string; host: string; path: string; query: string };

function parts(url: string): Parts {
  const found = /^([a-zA-Z][a-zA-Z0-9+.-]*):\/\/([^/?#]*)([^?#]*)(?:\?([^#]*))?/.exec(url);
  if (!found) throw new Error(`${url}: not an absolute address`);
  const scheme = found[1].toLowerCase();
  let host = found[2].replace(/^[^@]*@/, "").toLowerCase();
  if ((scheme === "https" && host.endsWith(":443")) || (scheme === "http" && host.endsWith(":80"))) {
    host = host.replace(/:\d+$/, "");
  }
  return { scheme, host, path: found[3], query: found[4] ?? "" };
}

function canonicalPath(path: string, service: string): string {
  const shown = path === "" ? "/" : path;
  if (service === "s3") return shown;
  return shown
    .split("/")
    .map((segment) => encode(encode(decoded(segment))))
    .join("/");
}

const queryPart = (text: string) => decoded(text.replace(/\+/g, " "));

function canonicalQuery(query: string): string {
  const pairs = query
    .split("&")
    .filter((pair) => pair !== "")
    .map((pair) => {
      const at = pair.indexOf("=");
      const [key, value] = at < 0 ? [pair, ""] : [pair.slice(0, at), pair.slice(at + 1)];
      return [encode(queryPart(key)), encode(queryPart(value))] as [string, string];
    });
  pairs.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : a[1] < b[1] ? -1 : a[1] > b[1] ? 1 : 0));
  return pairs.map(([key, value]) => `${key}=${value}`).join("&");
}

function canonicalHeaders(signing: Signing, host: string, added: [string, string][]): [string, string][] {
  const headers: [string, string][] = [...signing.headers, ...added].map(([name, value]) => [
    name.trim().toLowerCase(),
    value.split(/\s+/).filter((word) => word !== "").join(" "),
  ]);
  if (!headers.some(([name]) => name === "host")) headers.push(["host", host]);
  headers.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : a[1] < b[1] ? -1 : a[1] > b[1] ? 1 : 0));
  const merged: [string, string][] = [];
  for (const [name, value] of headers) {
    const last = merged[merged.length - 1];
    if (last && last[0] === name) last[1] = `${last[1]},${value}`;
    else merged.push([name, value]);
  }
  return merged;
}

export function sign(ctx, signing: Signing, keys: Keys): [string, string][] {
  const url = parts(signing.url);
  const added: [string, string][] = [["x-amz-date", signing.stamp]];
  if (keys.sessionToken) added.push(["x-amz-security-token", keys.sessionToken]);
  const headers = canonicalHeaders(signing, url.host, added);
  const declared = headers.find(([name]) => name === "x-amz-content-sha256");
  const payload = declared ? declared[1] : sha256(ctx, signing.body);
  const names = headers.map(([name]) => name).join(";");
  const listed = headers.map(([name, value]) => `${name}:${value}\n`).join("");
  const canonical = `${signing.method.toUpperCase()}\n${canonicalPath(url.path, signing.service)}\n${canonicalQuery(url.query)}\n${listed}\n${names}\n${payload}`;
  const signed = signature(ctx, keys.secretAccessKey, signing.stamp, signing.region, signing.service, canonical);
  const day = signing.stamp.slice(0, 8);
  const authorization = `${ALGORITHM} Credential=${keys.accessKeyId}/${day}/${signing.region}/${signing.service}/aws4_request, SignedHeaders=${names}, Signature=${signed}`;
  added.push(["authorization", authorization]);
  return added;
}
