import { attach, onEvent, send as command } from "./cdp.js";
import { fromBase64 } from "./results.js";

const LASTS_MS = 15 * 60 * 1000;
const watches = new Map();

export function watching(tabId) {
  return watches.has(tabId);
}

function texted(answer) {
  if (!answer) return "";
  return answer.base64Encoded ? new TextDecoder().decode(fromBase64(answer.body)) : String(answer.body ?? "");
}

async function backToApp(homes) {
  const tabs = await chrome.tabs.query({});
  const app = tabs.find((tab) => homes.some((origin) => String(tab.url || "").startsWith(origin)));
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

async function caught(tabId, params) {
  const watch = watches.get(tabId);
  const requestId = params.requestId;
  const code = params.responseStatusCode;
  if (!watch || params.responseErrorReason !== undefined || (code >= 300 && code < 400)) {
    await command(tabId, "Fetch.continueRequest", { requestId }).catch(() => {});
    return;
  }
  const body = await command(tabId, "Fetch.getResponseBody", { requestId }).catch(() => null);
  watch.report({ type: "captured", flow: watch.flow, url: params.request?.url || "", status: code, body: texted(body) });
  await command(tabId, "Fetch.failRequest", { requestId, errorReason: "BlockedByClient" }).catch(() => {});
  await ended(tabId, true);
  await backToApp(watch.homes);
}

const BINDING = "__signInLoopback";
const RESOLVER = "__signInLoopbackAnswer";
const SETTLES_MS = 1500;

export function shim(port) {
  return `(() => {
  if (globalThis.${RESOLVER}) return;
  const port = ${JSON.stringify(String(port))};
  const hosts = new Set(["localhost", "127.0.0.1", "[::1]"]);
  const pending = new Map();
  let next = 0;
  const ours = (given) => {
    try {
      const url = new URL(String(given), location.href);
      return url.protocol === "http:" && hosts.has(url.hostname) && url.port === port ? url : null;
    } catch {
      return null;
    }
  };
  Object.defineProperty(globalThis, ${JSON.stringify(RESOLVER)}, {
    value: (id, answer) => {
      const done = pending.get(id);
      pending.delete(id);
      if (done) done(answer || {});
    },
  });
  const original = globalThis.fetch;
  globalThis.fetch = async function (input, init) {
    const request = input instanceof Request ? input : null;
    const url = ours(request ? request.url : input);
    if (!url) return original.apply(this, arguments);
    const method = String(init?.method || request?.method || "GET").toUpperCase();
    const given = init?.body ?? (request ? await request.text() : "");
    const body = typeof given === "string" ? given : given == null ? "" : String(given);
    const answer = await new Promise((resolve) => {
      const id = ++next;
      pending.set(id, resolve);
      globalThis[${JSON.stringify(BINDING)}](JSON.stringify({ id, method, url: url.href, body }));
    });
    const status = Number.isInteger(answer.status) && answer.status >= 200 && answer.status <= 599 ? answer.status : 502;
    const empty = answer.body == null || status === 204 || status === 304;
    const text = typeof answer.body === "string";
    const content = empty ? null : text ? answer.body : JSON.stringify(answer.body);
    return new Response(content, { status, headers: { "content-type": text ? "text/plain" : "application/json" } });
  };
})();`;
}

async function originOf(tabId) {
  const tab = await chrome.tabs.get(tabId).catch(() => null);
  try {
    return new URL(String(tab?.url || "")).origin;
  } catch {
    return "";
  }
}

async function relayed(tabId, params) {
  const watch = watches.get(tabId);
  if (!watch?.ask || params.name !== BINDING) return;
  let sent;
  try {
    sent = JSON.parse(params.payload);
  } catch {
    return;
  }
  const request = { method: String(sent.method || "GET"), url: String(sent.url || ""), body: String(sent.body || "") };
  const answer = await watch.ask({ flow: watch.flow, origin: await originOf(tabId), request }).catch(() => ({ status: 502, body: null }));
  const reply = JSON.stringify({ status: answer?.status, body: answer?.body ?? null });
  const expression = `globalThis[${JSON.stringify(RESOLVER)}]?.(${Number(sent.id)}, ${reply})`;
  await command(tabId, "Runtime.evaluate", { expression, contextId: params.executionContextId }).catch(() => {});
  if (answer?.done !== true) return;
  setTimeout(() => {
    ended(tabId, true)
      .then(() => backToApp(watch.homes))
      .catch(() => {});
  }, SETTLES_MS);
}

export function listen() {
  onEvent((tabId, method, params) => {
    if (method === "Fetch.requestPaused" && watches.has(tabId)) caught(tabId, params).catch(() => ended(tabId, false));
    if (method === "Runtime.bindingCalled" && watches.has(tabId)) relayed(tabId, params).catch(() => {});
  });
  chrome.tabs.onRemoved.addListener((tabId) => {
    const watch = watches.get(tabId);
    if (watch) clearTimeout(watch.timer);
    watches.delete(tabId);
  });
}

export function renames(given) {
  return (Array.isArray(given) ? given : [])
    .filter((rule) => rule && typeof rule.find === "string" && rule.find && typeof rule.replace === "string")
    .map((rule) => [rule.find, rule.replace]);
}

export function renamer(rules) {
  return `(() => {
  const rules = ${JSON.stringify(rules)}.map(([find, replace]) => [new RegExp(find, "gi"), replace]);
  const fix = (root) => {
    if (!root) return;
    if (root.nodeType === 3) {
      let text = root.nodeValue;
      for (const [find, replace] of rules) text = text.replace(find, replace);
      if (text !== root.nodeValue) root.nodeValue = text;
      return;
    }
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) fix(node);
  };
  const start = () => {
    fix(document.body);
    new MutationObserver((changes) => {
      for (const change of changes) {
        if (change.type === "characterData") fix(change.target);
        change.addedNodes.forEach(fix);
      }
    }).observe(document.body, { childList: true, subtree: true, characterData: true });
  };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})();`;
}

async function listening(message, link) {
  const url = String(message.url || "");
  const port = Number(message.listen);
  if (!/^https:\/\//i.test(url) || !Number.isInteger(port) || port <= 0 || port > 65535 || !link.ask) return;
  const tab = await chrome.tabs.create({ url: "about:blank", active: true });
  const timer = setTimeout(() => ended(tab.id, false), LASTS_MS);
  watches.set(tab.id, { flow: String(message.flow || ""), timer, report: link.send, ask: link.ask, homes: link.homes });
  try {
    await attach(tab.id);
    await command(tab.id, "Runtime.addBinding", { name: BINDING });
    await command(tab.id, "Page.addScriptToEvaluateOnNewDocument", { source: shim(port) });
  } catch {
    await ended(tab.id, false);
  }
  await chrome.tabs.update(tab.id, { url });
}

export async function watch(message, link) {
  if (message.listen !== undefined) return listening(message, link);
  const patterns = (Array.isArray(message.capture) ? message.capture : []).map(String).filter(Boolean);
  const url = String(message.url || "");
  if (!/^https:\/\//i.test(url) || patterns.length === 0) return;
  const tab = await chrome.tabs.create({ url: "about:blank", active: true });
  const timer = setTimeout(() => ended(tab.id, false), LASTS_MS);
  watches.set(tab.id, { flow: String(message.flow || ""), timer, report: link.send, homes: link.homes });
  try {
    await attach(tab.id);
    await command(tab.id, "Fetch.enable", { patterns: patterns.map((urlPattern) => ({ urlPattern, requestStage: "Response" })) });
    const rules = renames(message.rename);
    if (rules.length > 0) {
      await command(tab.id, "Page.enable", {}).catch(() => {});
      await command(tab.id, "Page.addScriptToEvaluateOnNewDocument", { source: renamer(rules) }).catch(() => {});
    }
  } catch {
    await ended(tab.id, false);
  }
  await chrome.tabs.update(tab.id, { url });
}
