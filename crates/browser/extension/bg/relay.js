import { ask } from "./ask.js";

const TRUSTED_KEY = "relayOrigins";
const LIFETIME_MINUTES = 15;
const WATCHED_KEY = "relayWatched";
const ALARM_PREFIX = "relay:";
const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);
const asking = new Map();

function parsed(value) {
  try {
    return new URL(String(value));
  } catch {
    return null;
  }
}

export function relayTarget(from, to, page) {
  const source = parsed(from);
  const target = parsed(to);
  const origin = parsed(page);
  if (!source || !target || !origin) return { reason: "bad-url" };
  if (source.protocol !== "http:" || !LOOPBACK.has(source.hostname) || source.search || source.hash) return { reason: "bad-from" };
  if (target.origin !== origin.origin || target.search || target.hash) return { reason: "bad-to" };
  if (target.protocol !== "https:" && !(target.protocol === "http:" && LOOPBACK.has(origin.hostname))) return { reason: "bad-to" };
  return { from: source.href, to: target.href, origin: origin.origin };
}

export function escaped(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export function filterOf(from) {
  return `^${escaped(from)}(.*)$`;
}

export function ruleOf(id, from, to, tabId) {
  return {
    id,
    priority: 1,
    action: { type: "redirect", redirect: { regexSubstitution: `${to}\\1` } },
    condition: { regexFilter: filterOf(from), resourceTypes: ["main_frame"], tabIds: [tabId] },
  };
}

export function signInUrl(value) {
  const url = parsed(value);
  return url && url.protocol === "https:" ? url.href : null;
}

async function trusted(origin) {
  const stored = await chrome.storage.local.get(TRUSTED_KEY);
  return Array.isArray(stored[TRUSTED_KEY]) && stored[TRUSTED_KEY].includes(origin);
}

export async function trust(origin) {
  const stored = await chrome.storage.local.get(TRUSTED_KEY);
  const known = Array.isArray(stored[TRUSTED_KEY]) ? stored[TRUSTED_KEY] : [];
  if (!known.includes(origin)) await chrome.storage.local.set({ [TRUSTED_KEY]: [...known, origin] });
}

async function approved(origin) {
  if (await trusted(origin)) return true;
  if (!asking.has(origin)) {
    const question = ask({ kind: "relay", origin }).then(async (answer) => {
      if (answer.choice !== "allow") return false;
      await trust(origin);
      return true;
    });
    asking.set(origin, question.finally(() => asking.delete(origin)));
  }
  return asking.get(origin);
}

async function drop(ids) {
  if (ids.length === 0) return;
  await chrome.declarativeNetRequest.updateSessionRules({ removeRuleIds: ids });
  await Promise.all(ids.map((id) => chrome.alarms.clear(`${ALARM_PREFIX}${id}`)));
}

async function install(from, to, tabId, back) {
  const rules = await chrome.declarativeNetRequest.getSessionRules();
  const id = rules.reduce((top, rule) => Math.max(top, rule.id), 0) + 1;
  await chrome.declarativeNetRequest.updateSessionRules({ addRules: [ruleOf(id, from, to, tabId)] });
  await chrome.alarms.create(`${ALARM_PREFIX}${id}`, { delayInMinutes: LIFETIME_MINUTES });
  const kept = await watched();
  kept[id] = { tab: tabId, back, to, arrived: false };
  await chrome.storage.session.set({ [WATCHED_KEY]: kept });
}

async function watched() {
  const saved = await chrome.storage.session.get(WATCHED_KEY);
  return saved[WATCHED_KEY] || {};
}

async function forget(ids) {
  const kept = await watched();
  for (const id of ids) delete kept[id];
  await chrome.storage.session.set({ [WATCHED_KEY]: kept });
  await drop(ids.map(Number));
}

export function pageOf(sender) {
  if (sender?.id !== chrome.runtime.id || !sender.tab || sender.frameId !== 0) return null;
  return sender.origin || sender.tab.url || null;
}

export async function relay(message, sender) {
  const page = pageOf(sender);
  if (!page) return { ok: false, reason: "bad-sender" };
  const target = relayTarget(message.from, message.to, page);
  if (target.reason) return { ok: false, reason: target.reason };
  const url = signInUrl(message.url);
  if (!url) return { ok: false, reason: "bad-url" };
  if (!(await approved(target.origin))) return { ok: false, reason: "declined" };
  try {
    const tab = await chrome.tabs.create({ url: "about:blank", active: true, index: sender.tab.index + 1, windowId: sender.tab.windowId });
    await install(target.from, target.to, tab.id, sender.tab.id);
    await chrome.tabs.update(tab.id, { url });
    return { ok: true, opened: true };
  } catch {
    return { ok: false, reason: "rule-failed" };
  }
}

async function updated(tabId, change) {
  const kept = await watched();
  for (const [id, watch] of Object.entries(kept)) {
    if (watch.tab !== tabId) continue;
    if (change.url && change.url.startsWith(watch.to)) {
      watch.arrived = true;
      await chrome.storage.session.set({ [WATCHED_KEY]: kept });
    }
    if (watch.arrived && change.status === "complete") {
      await forget([id]);
      await chrome.tabs.remove(tabId).catch(() => {});
      await chrome.tabs.update(watch.back, { active: true }).catch(() => {});
    }
  }
}

async function closed(tabId) {
  const kept = await watched();
  await forget(Object.keys(kept).filter((id) => kept[id].tab === tabId));
}

export function watchRelays() {
  chrome.alarms.onAlarm.addListener((alarm) => {
    if (!alarm.name.startsWith(ALARM_PREFIX)) return;
    const id = Number(alarm.name.slice(ALARM_PREFIX.length));
    if (Number.isInteger(id)) forget([String(id)]).catch(() => {});
  });
  chrome.tabs.onUpdated.addListener((tabId, change) => {
    if (change.url || change.status === "complete") updated(tabId, change).catch(() => {});
  });
  chrome.tabs.onRemoved.addListener((tabId) => closed(tabId).catch(() => {}));
}
