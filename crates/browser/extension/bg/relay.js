import { ask } from "./ask.js";

const TRUSTED_KEY = "relayOrigins";
const LIFETIME_MINUTES = 15;
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

export function ruleOf(id, from, to) {
  return {
    id,
    priority: 1,
    action: { type: "redirect", redirect: { regexSubstitution: `${to}\\1` } },
    condition: { regexFilter: filterOf(from), resourceTypes: ["main_frame"] },
  };
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

async function install(from, to) {
  const rules = await chrome.declarativeNetRequest.getSessionRules();
  const filter = filterOf(from);
  await drop(rules.filter((rule) => rule.condition?.regexFilter === filter).map((rule) => rule.id));
  const id = rules.reduce((top, rule) => Math.max(top, rule.id), 0) + 1;
  await chrome.declarativeNetRequest.updateSessionRules({ addRules: [ruleOf(id, from, to)] });
  await chrome.alarms.create(`${ALARM_PREFIX}${id}`, { delayInMinutes: LIFETIME_MINUTES });
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
  if (!(await approved(target.origin))) return { ok: false, reason: "declined" };
  try {
    await install(target.from, target.to);
    return { ok: true };
  } catch {
    return { ok: false, reason: "rule-failed" };
  }
}

async function arrived(url) {
  const rules = await chrome.declarativeNetRequest.getSessionRules();
  const used = rules.filter((rule) => {
    const to = String(rule.action?.redirect?.regexSubstitution || "").replace(/\\1$/, "");
    return to && url.startsWith(to);
  });
  await drop(used.map((rule) => rule.id));
}

export function watchRelays() {
  chrome.alarms.onAlarm.addListener((alarm) => {
    if (!alarm.name.startsWith(ALARM_PREFIX)) return;
    const id = Number(alarm.name.slice(ALARM_PREFIX.length));
    if (Number.isInteger(id)) chrome.declarativeNetRequest.updateSessionRules({ removeRuleIds: [id] }).catch(() => {});
  });
  chrome.tabs.onUpdated.addListener((_tabId, change) => {
    if (change.url) arrived(change.url).catch(() => {});
  });
}
