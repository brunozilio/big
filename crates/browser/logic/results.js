export class ToolError extends Error {}

export const text = (body) => ({ type: "text", text: body });

export const image = (data, mimeType = "image/jpeg") => ({ type: "image", data, mimeType });

export function ok(...parts) {
  return { content: parts.flat().filter(Boolean).map((part) => (typeof part === "string" ? text(part) : part)), isError: false };
}

export function fail(message) {
  return { content: [text(message)], isError: true };
}

export function refuse(message) {
  throw new ToolError(message);
}

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export function toBase64(buffer) {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let at = 0; at < bytes.length; at += 0x8000) {
    binary += String.fromCharCode.apply(null, bytes.subarray(at, at + 0x8000));
  }
  return btoa(binary);
}

export function fromBase64(data) {
  const binary = atob(data);
  const bytes = new Uint8Array(binary.length);
  for (let at = 0; at < binary.length; at += 1) bytes[at] = binary.charCodeAt(at);
  return bytes;
}

export async function blobToBase64(blob) {
  return toBase64(await blob.arrayBuffer());
}

export function clip(value, most) {
  const body = String(value ?? "");
  return body.length > most ? `${body.slice(0, most)}…` : body;
}

export function number(value, name) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) refuse(`\`${name}\` must be a number.`);
  return parsed;
}
