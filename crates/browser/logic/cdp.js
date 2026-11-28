import { refuse } from "./results.js";

const PROTOCOL = "1.3";
const attached = new Set();
const attaching = new Map();
const listeners = new Set();
const attachListeners = new Set();

export function onEvent(listener) {
  listeners.add(listener);
}

export function onAttach(listener) {
  attachListeners.add(listener);
}

chrome.debugger.onEvent.addListener((source, method, params) => {
  if (source.tabId === undefined) return;
  for (const listener of listeners) {
    try {
      listener(source.tabId, method, params || {});
    } catch {}
  }
});

chrome.debugger.onDetach.addListener((source) => {
  if (source.tabId !== undefined) attached.delete(source.tabId);
});

chrome.tabs.onRemoved.addListener((tabId) => attached.delete(tabId));

export async function attach(tabId) {
  if (attached.has(tabId)) return;
  if (attaching.has(tabId)) return attaching.get(tabId);
  const work = (async () => {
    try {
      await chrome.debugger.attach({ tabId }, PROTOCOL);
    } catch (error) {
      const message = String(error?.message || error);
      if (!message.includes("Another debugger is already attached")) {
        refuse(`Could not control tab ${tabId}: ${message}`);
      }
      refuse(
        `Tab ${tabId} is already being debugged by another tool (DevTools or another extension). Close DevTools on that tab, or use another tab.`,
      );
    }
    attached.add(tabId);
    await Promise.allSettled([
      raw(tabId, "Page.enable"),
      raw(tabId, "Runtime.enable"),
      raw(tabId, "Log.enable"),
      raw(tabId, "Network.enable", { maxPostDataSize: 0 }),
    ]);
    await raw(tabId, "Network.setBypassServiceWorker", { bypass: true }).catch(() => {});
    for (const listener of attachListeners) {
      try {
        await listener(tabId);
      } catch {}
    }
  })();
  attaching.set(tabId, work);
  try {
    await work;
  } finally {
    attaching.delete(tabId);
  }
}

function raw(tabId, method, params = {}) {
  return chrome.debugger.sendCommand({ tabId }, method, params);
}

export async function send(tabId, method, params = {}) {
  await attach(tabId);
  try {
    return await raw(tabId, method, params);
  } catch (error) {
    const message = String(error?.message || error);
    if (message.includes("Detached") || message.includes("not attached")) {
      attached.delete(tabId);
      await attach(tabId);
      return raw(tabId, method, params);
    }
    throw error;
  }
}

export async function evaluate(tabId, expression) {
  const answer = await send(tabId, "Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (answer.exceptionDetails) refuse(answer.exceptionDetails.exception?.description || answer.exceptionDetails.text);
  return answer.result?.value;
}

export function isAttached(tabId) {
  return attached.has(tabId);
}
