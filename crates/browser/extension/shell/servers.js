import { ask } from "./ask.js";
import { homes, identity, rememberServer } from "./identity.js";
import { pageOf, trust } from "./relay.js";

const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);

function parsed(value) {
  try {
    return new URL(String(value));
  } catch {
    return null;
  }
}

export function serverBase(server, page) {
  const target = parsed(server);
  const origin = parsed(page);
  if (!target || !origin || target.search || target.hash || target.origin !== origin.origin) return null;
  if (target.protocol !== "https:" && !(target.protocol === "http:" && LOOPBACK.has(target.hostname))) return null;
  return target.href.replace(/\/+$/, "");
}

export async function connectServer(message, sender) {
  const page = pageOf(sender);
  const base = page && serverBase(message.server, page);
  const code = String(message.code || "");
  if (!base || !code) return { ok: false, reason: "bad-server" };
  const origin = new URL(base).origin;
  const answer = await ask({ kind: "server", origin, name: (await identity()).name });
  if (answer.choice !== "connect") return { ok: false, reason: "declined" };
  const name = String(answer.name || "").trim();
  if (name) await chrome.storage.local.set({ deviceName: name });
  const me = await identity();
  const response = await fetch(`${base}/api/browser/claim`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ code, deviceId: me.deviceId, name: me.name }),
  }).catch(() => null);
  if (!response?.ok) return { ok: false, reason: "claim-failed" };
  const claimed = await response.json().catch(() => ({}));
  if (!claimed.token) return { ok: false, reason: "claim-failed" };
  await trust(origin);
  await rememberServer(base, claimed.token, typeof claimed.logicKey === "string" ? claimed.logicKey : null);
  chrome.runtime.sendMessage({ type: "servers-changed" }).catch(() => {});
  return { ok: true };
}

export async function reloadFor(_message, sender) {
  const page = pageOf(sender);
  const origin = page && parsed(page)?.origin;
  if (!origin || !(await homes()).includes(origin)) return { ok: false, reason: "unknown-page" };
  setTimeout(() => chrome.runtime.reload(), 200);
  return { ok: true };
}
