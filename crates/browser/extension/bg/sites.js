import { refuse } from "./results.js";

const ANSWER_WAIT_MS = 120000;
const allowedOnce = new Set();
const waiting = new Map();

export function hostOf(url) {
  try {
    const parsed = new URL(url);
    return ["http:", "https:"].includes(parsed.protocol) ? parsed.hostname : "";
  } catch {
    return "";
  }
}

export async function settings() {
  const saved = await chrome.storage.local.get(["siteMode", "sites"]);
  return { mode: saved.siteMode || "ask", sites: saved.sites || {} };
}

function covered(sites, host) {
  const parts = host.split(".");
  for (let at = 0; at < parts.length - 1; at += 1) {
    const candidate = parts.slice(at).join(".");
    if (sites[candidate]) return sites[candidate];
  }
  return null;
}

export async function ensure(url, tool, summary) {
  const host = hostOf(url);
  if (!host) return;
  const { mode, sites } = await settings();
  const decided = covered(sites, host);
  if (decided === "deny") refuse(denied(host));
  if (mode === "all" || decided === "allow" || allowedOnce.has(host)) return;
  const answer = await ask({ kind: "site", host, tool, summary: summary || "" });
  if (answer.choice === "always") {
    await chrome.storage.local.set({ sites: { ...(await settings()).sites, [host]: "allow" } });
    return;
  }
  if (answer.choice === "once") {
    allowedOnce.add(host);
    return;
  }
  if (answer.choice === "never") await chrome.storage.local.set({ sites: { ...(await settings()).sites, [host]: "deny" } });
  refuse(answer.choice === "timeout" ? `Nobody answered the permission request for ${host} within ${ANSWER_WAIT_MS / 1000} seconds. Ask the user to allow the site in the browser, then try again.` : denied(host));
}

function denied(host) {
  return `The user declined permission for ${host}. Do not retry this site; tell the user and move on.`;
}

export async function ask(question, id = crypto.randomUUID()) {
  const params = new URLSearchParams({ id, ...question });
  const window = await chrome.windows.create({ url: chrome.runtime.getURL(`ui/ask.html?${params}`), type: "popup", width: 460, height: 380, focused: true });
  return new Promise((resolve) => {
    const finish = (answer) => {
      if (!waiting.has(id)) return;
      waiting.delete(id);
      clearTimeout(timer);
      chrome.windows.onRemoved.removeListener(closed);
      chrome.windows.remove(window.id).catch(() => {});
      resolve(answer);
    };
    const closed = (windowId) => {
      if (windowId === window.id) finish({ choice: "decline" });
    };
    const timer = setTimeout(() => finish({ choice: "timeout" }), ANSWER_WAIT_MS);
    waiting.set(id, finish);
    chrome.windows.onRemoved.addListener(closed);
  });
}

export function answered(id, answer) {
  waiting.get(id)?.(answer);
}

export function cancel(id) {
  waiting.get(id)?.({ choice: "timeout" });
}

export function forgetOnce() {
  allowedOnce.clear();
}
