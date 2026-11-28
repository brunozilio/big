import { onEvent } from "./cdp.js";
import { clip, refuse } from "./results.js";

const MOST_KEPT = 2000;
const consoles = new Map();
const networks = new Map();
const hosts = new Map();
const SECRET_KEYS = /^(access_?token|id_?token|refresh_?token|token|auth|authorization|api_?key|apikey|key|secret|client_?secret|password|passwd|pwd|sig|signature|code|session|sessionid|sid|jwt|x-amz-signature|x-amz-credential|x-amz-security-token)$/i;

function hostOf(url) {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

function base(host) {
  return host.split(".").slice(-2).join(".");
}

function list(map, tabId) {
  if (!map.has(tabId)) map.set(tabId, []);
  return map.get(tabId);
}

function keep(entries, entry) {
  entries.push(entry);
  if (entries.length > MOST_KEPT) entries.splice(0, entries.length - MOST_KEPT);
}

function argument(arg) {
  if (arg.type === "string") return arg.value;
  if (arg.unserializableValue) return arg.unserializableValue;
  if (arg.value !== undefined) return typeof arg.value === "object" ? JSON.stringify(arg.value) : String(arg.value);
  if (arg.preview?.properties) {
    const shown = arg.preview.properties.map((p) => `${p.name}: ${p.type === "string" ? JSON.stringify(p.value) : p.value}`).join(", ");
    const open = arg.preview.subtype === "array" ? "[" : "{";
    const close = arg.preview.subtype === "array" ? "]" : "}";
    return `${open}${shown}${arg.preview.overflow ? ", …" : ""}${close}`;
  }
  return arg.description || `[${arg.type}]`;
}

function frameOf(trace) {
  const top = trace?.callFrames?.[0];
  return top ? `${top.url}:${top.lineNumber + 1}` : "";
}

onEvent((tabId, method, params) => {
  if (method === "Runtime.consoleAPICalled") {
    const level = { warning: "warn", assert: "error" }[params.type] || params.type;
    keep(list(consoles, tabId), { at: params.timestamp, level, text: (params.args || []).map(argument).join(" "), source: frameOf(params.stackTrace), host: hosts.get(tabId) || "" });
  } else if (method === "Runtime.exceptionThrown") {
    const details = params.exceptionDetails || {};
    const text = details.exception?.description || details.text || "Uncaught exception";
    const source = details.url ? `${details.url}:${(details.lineNumber || 0) + 1}` : frameOf(details.stackTrace);
    keep(list(consoles, tabId), { at: params.timestamp, level: "exception", text, source, host: hosts.get(tabId) || "" });
  } else if (method === "Log.entryAdded") {
    const entry = params.entry || {};
    keep(list(consoles, tabId), { at: entry.timestamp, level: entry.level === "warning" ? "warn" : entry.level, text: entry.text, source: entry.url || "", host: hosts.get(tabId) || "" });
  } else if (method === "Page.frameNavigated" && !params.frame?.parentId) {
    const host = hostOf(params.frame?.url || "");
    const before = hosts.get(tabId);
    if (before !== undefined && base(before) !== base(host)) networks.set(tabId, []);
    hosts.set(tabId, host);
  } else {
    network(tabId, method, params);
  }
});

function network(tabId, method, params) {
  if (method === "Network.requestWillBeSent") {
    keep(list(networks, tabId), {
      id: params.requestId,
      method: params.request?.method || "GET",
      url: params.request?.url || "",
      type: (params.type || "other").toLowerCase(),
      started: params.timestamp,
      status: null,
    });
    return;
  }
  const entry = list(networks, tabId).findLast((item) => item.id === params.requestId);
  if (!entry) return;
  if (method === "Network.responseReceived") {
    entry.status = params.response?.status;
    entry.statusText = params.response?.statusText;
    entry.mime = params.response?.mimeType;
  } else if (method === "Network.loadingFinished") {
    entry.size = params.encodedDataLength;
    entry.ms = Math.round((params.timestamp - entry.started) * 1000);
    entry.done = true;
  } else if (method === "Network.loadingFailed") {
    entry.failed = params.canceled ? "canceled" : params.errorText || "failed";
    entry.done = true;
  }
}

export function redact(url) {
  try {
    const parsed = new URL(url);
    if (parsed.username) parsed.username = "REDACTED";
    if (parsed.password) parsed.password = "REDACTED";
    for (const key of [...parsed.searchParams.keys()]) {
      if (SECRET_KEYS.test(key)) parsed.searchParams.set(key, "REDACTED");
    }
    return parsed.toString();
  } catch {
    return url;
  }
}

function pattern(source) {
  if (!source) return null;
  try {
    return new RegExp(source, "i");
  } catch (error) {
    refuse(`\`pattern\` is not a valid regular expression: ${error.message}`);
  }
}

function time(seconds) {
  const date = seconds > 1e12 ? new Date(seconds) : new Date(seconds * 1000);
  return date.toISOString().slice(11, 23);
}

export function readConsole(tab, input) {
  const host = hostOf(tab.url || "");
  const matcher = pattern(input.pattern);
  const limit = Math.max(1, Number(input.limit) || 100);
  const all = consoles.get(tab.id) || [];
  const found = all
    .filter((entry) => !entry.host || !host || base(entry.host) === base(host))
    .filter((entry) => !input.onlyErrors || entry.level === "error" || entry.level === "exception")
    .filter((entry) => !matcher || matcher.test(entry.text) || matcher.test(entry.source));
  if (input.clear) consoles.set(tab.id, []);
  const filters = [input.pattern ? `pattern /${input.pattern}/` : "", input.onlyErrors ? "errors only" : ""].filter(Boolean).join(", ");
  if (found.length === 0) {
    return `No console messages found${filters ? ` (${filters})` : ""} for ${host || "this tab"}. Messages are recorded from when the browser tools first attached to the tab; reload the page to capture what it logs while loading.`;
  }
  const shown = found.slice(-limit);
  const lines = shown.map((entry) => `[${time(entry.at)}] [${entry.level.toUpperCase()}] ${clip(entry.text, 2000)}${entry.source ? ` (${entry.source})` : ""}`);
  const more = found.length > shown.length ? ` (showing the last ${shown.length})` : "";
  return `Found ${found.length} console message${found.length === 1 ? "" : "s"}${filters ? ` matching ${filters}` : ""}${more}:\n\n${lines.join("\n")}`;
}

function size(bytes) {
  if (bytes === undefined) return "";
  return bytes < 1024 ? `${bytes} B` : bytes < 1048576 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1048576).toFixed(1)} MB`;
}

export function readNetwork(tab, input) {
  const limit = Math.max(1, Number(input.limit) || 100);
  const all = networks.get(tab.id) || [];
  const found = all.map((entry) => ({ ...entry, shownUrl: redact(entry.url) })).filter((entry) => !input.urlPattern || entry.shownUrl.includes(input.urlPattern));
  if (input.clear) networks.set(tab.id, []);
  if (found.length === 0) {
    return `No network requests found${input.urlPattern ? ` containing "${input.urlPattern}"` : ""}. Requests are recorded from when the browser tools first attached to the tab and cleared when it moves to another domain; reload the page to capture its requests.`;
  }
  const shown = found.slice(-limit);
  const lines = shown.map((entry, index) => {
    const outcome = entry.failed ? `failed: ${entry.failed}` : entry.status ? `${entry.status}${entry.statusText ? ` ${entry.statusText}` : ""}` : "pending";
    const details = [entry.type, entry.mime, size(entry.size), entry.ms !== undefined ? `${entry.ms} ms` : ""].filter(Boolean).join(", ");
    return `${index + 1}. ${entry.method} ${clip(entry.shownUrl, 500)} → ${outcome}${details ? ` (${details})` : ""}`;
  });
  const more = found.length > shown.length ? ` (showing the last ${shown.length})` : "";
  return `Found ${found.length} network request${found.length === 1 ? "" : "s"}${more}:\n\n${lines.join("\n")}`;
}

export function forget(tabId) {
  consoles.delete(tabId);
  networks.delete(tabId);
  hosts.delete(tabId);
}
