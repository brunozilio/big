export const SCHEME = "VERACODE-HMAC-SHA-256";
const VERSION = "vcode_request_version_1";

export type Keys = { apiId: string; apiKey: string };

export const unprefixed = (text: string) => {
  const at = text.indexOf("-");
  return at < 0 ? text : text.slice(at + 1);
};

export function readKeys(token: string): Keys | undefined {
  let parsed;
  try {
    parsed = JSON.parse(token);
  } catch (_) {
    return undefined;
  }
  if (typeof parsed?.apiId !== "string" || typeof parsed?.apiKey !== "string") return undefined;
  const apiId = unprefixed(parsed.apiId.trim());
  const apiKey = unprefixed(parsed.apiKey.trim());
  return apiId !== "" && apiKey !== "" ? { apiId, apiKey } : undefined;
}

export type Signed = { method: string; host: string; pathAndQuery: string };

const chained = (ctx, key: string, data: string, dataEncoding: string) =>
  ctx.crypto.hmac({ key, keyEncoding: "hex", data, dataEncoding, output: "hex" });

export function signature(ctx, keys: Keys, request: Signed, millis: number, nonce: string): string | undefined {
  if (keys.apiKey.length % 2 !== 0) return undefined;
  const data = `id=${keys.apiId.toLowerCase()}&host=${request.host.toLowerCase()}&url=${request.pathAndQuery}&method=${request.method.toUpperCase()}`;
  try {
    const keyNonce = chained(ctx, keys.apiKey, nonce, "hex");
    const keyDate = chained(ctx, keyNonce, String(millis), "text");
    const keySignature = chained(ctx, keyDate, VERSION, "text");
    return chained(ctx, keySignature, data, "text");
  } catch (_) {
    return undefined;
  }
}

export function header(ctx, keys: Keys, request: Signed, millis: number, nonce: string): string | undefined {
  const signed = signature(ctx, keys, request, millis, nonce);
  return signed === undefined ? undefined : `${SCHEME} id=${keys.apiId},ts=${millis},nonce=${nonce},sig=${signed}`;
}
