import { PORT } from "../config.js";

const SERVERS_KEY = "servers";

export async function identity() {
  const saved = await chrome.storage.local.get(["deviceId", "deviceName"]);
  let deviceId = saved.deviceId;
  if (!deviceId) {
    deviceId = crypto.randomUUID();
    await chrome.storage.local.set({ deviceId });
  }
  const info = await chrome.runtime.getPlatformInfo();
  return { deviceId, name: saved.deviceName || defaultName(info.os), platform: info.os };
}

function defaultName(os) {
  const systems = { mac: "Mac", win: "Windows", linux: "Linux", cros: "ChromeOS" };
  return `Chrome on ${systems[os] || os}`;
}

export async function servers() {
  const saved = await chrome.storage.local.get(SERVERS_KEY);
  return Array.isArray(saved[SERVERS_KEY]) ? saved[SERVERS_KEY] : [];
}

export async function rememberServer(base, token) {
  const kept = (await servers()).filter((server) => server.base !== base);
  await chrome.storage.local.set({ [SERVERS_KEY]: [...kept, { base, token }] });
}

export async function forgetServer(base) {
  await chrome.storage.local.set({ [SERVERS_KEY]: (await servers()).filter((server) => server.base !== base) });
}

export function localHomes() {
  return PORT ? [`http://127.0.0.1:${PORT}`, `http://localhost:${PORT}`] : [];
}

export async function homes() {
  return [...localHomes(), ...(await servers()).map((server) => new URL(server.base).origin)];
}
