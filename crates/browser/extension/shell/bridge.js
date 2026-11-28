const ALLOWED = new Set(["tabs", "tabGroups", "windows", "debugger", "storage", "downloads", "alarms", "runtime", "scripting"]);
const SHUT = new Set(["runtime.sendMessage", "runtime.connect", "runtime.onMessage"]);

const EVENTS = [
  ["debugger.onEvent", () => chrome.debugger.onEvent],
  ["debugger.onDetach", () => chrome.debugger.onDetach],
  ["tabs.onRemoved", () => chrome.tabs.onRemoved],
  ["tabs.onUpdated", () => chrome.tabs.onUpdated],
  ["tabs.onCreated", () => chrome.tabs.onCreated],
  ["windows.onRemoved", () => chrome.windows.onRemoved],
  ["alarms.onAlarm", () => chrome.alarms.onAlarm],
];

export function resolved(api, root = chrome) {
  const parts = String(api).split(".");
  if (parts.length < 2 || !ALLOWED.has(parts[0]) || SHUT.has(api)) throw new Error(`The browser bridge does not offer ${api}.`);
  let owner = root;
  for (const part of parts.slice(0, -1)) owner = owner?.[part];
  const run = owner?.[parts[parts.length - 1]];
  if (typeof run !== "function") throw new Error(`The browser bridge does not offer ${api}.`);
  return { owner, run };
}

export async function invoke(api, args) {
  const { owner, run } = resolved(api);
  return run.apply(owner, Array.isArray(args) ? args : []);
}

export function forwardEvents(deliver) {
  for (const [name, source] of EVENTS) {
    source().addListener((...args) => deliver(name, args));
  }
}
