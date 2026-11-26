import { PORT } from "../config.js";
import { attach, onEvent, send as command } from "./cdp.js";
import { fromBase64 } from "./results.js";

const LASTS_MS = 15 * 60 * 1000;
const APP_ORIGINS = [`http://127.0.0.1:${PORT}`, `http://localhost:${PORT}`];
const watches = new Map();

export function watching(tabId) {
  return watches.has(tabId);
}

function texted(answer) {
  if (!answer) return "";
  return answer.base64Encoded ? new TextDecoder().decode(fromBase64(answer.body)) : String(answer.body ?? "");
}

async function backToApp() {
  const tabs = await chrome.tabs.query({});
  const app = tabs.find((tab) => APP_ORIGINS.some((origin) => String(tab.url || "").startsWith(origin)));
  if (!app) return;
  await chrome.tabs.update(app.id, { active: true }).catch(() => {});
  await chrome.windows.update(app.windowId, { focused: true }).catch(() => {});
}

async function ended(tabId, close) {
  const watch = watches.get(tabId);
  if (!watch) return;
  watches.delete(tabId);
  clearTimeout(watch.timer);
  await chrome.debugger.detach({ tabId }).catch(() => {});
  if (close) await chrome.tabs.remove(tabId).catch(() => {});
}

async function caught(tabId, params, report) {
  const watch = watches.get(tabId);
  const requestId = params.requestId;
  const code = params.responseStatusCode;
  if (!watch || params.responseErrorReason !== undefined || (code >= 300 && code < 400)) {
    await command(tabId, "Fetch.continueRequest", { requestId }).catch(() => {});
    return;
  }
  const body = await command(tabId, "Fetch.getResponseBody", { requestId }).catch(() => null);
  report({ type: "captured", flow: watch.flow, url: params.request?.url || "", status: code, body: texted(body) });
  await command(tabId, "Fetch.failRequest", { requestId, errorReason: "BlockedByClient" }).catch(() => {});
  await ended(tabId, true);
  await backToApp();
}

export function listen(report) {
  onEvent((tabId, method, params) => {
    if (method === "Fetch.requestPaused" && watches.has(tabId)) caught(tabId, params, report).catch(() => ended(tabId, false));
  });
  chrome.tabs.onRemoved.addListener((tabId) => {
    const watch = watches.get(tabId);
    if (watch) clearTimeout(watch.timer);
    watches.delete(tabId);
  });
}

export async function watch(message) {
  const patterns = (Array.isArray(message.capture) ? message.capture : []).map(String).filter(Boolean);
  const url = String(message.url || "");
  if (!/^https:\/\//i.test(url) || patterns.length === 0) return;
  const tab = await chrome.tabs.create({ url: "about:blank", active: true });
  const timer = setTimeout(() => ended(tab.id, false), LASTS_MS);
  watches.set(tab.id, { flow: String(message.flow || ""), timer });
  try {
    await attach(tab.id);
    await command(tab.id, "Fetch.enable", { patterns: patterns.map((urlPattern) => ({ urlPattern, requestStage: "Response" })) });
  } catch {
    await ended(tab.id, false);
  }
  await chrome.tabs.update(tab.id, { url });
}
