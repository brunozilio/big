import { BUILD, LOGIC_KEY, PORT, SOCKET, VERSION } from "../config.js";

const RETRY_MS = 3000;
const LOCAL = "local";
const links = new Map();
const SANDBOX_KINDS = new Set(["chrome", "ask", "cancel", "identity", "loopback"]);

async function asked(link, message) {
  if (!link.ask) return { status: 502, body: null };
  return link.ask(message);
}

export async function worker(type, extra = {}) {
  const answer = await chrome.runtime.sendMessage({ type, ...extra });
  if (answer?.error) throw new Error(answer.error);
  return answer?.value;
}

function showState() {
  const states = [...links.values()].map((link) => link.state);
  const state = states.includes("connected") ? "connected" : states.includes("connecting") ? "connecting" : "disconnected";
  worker("badge", { state }).catch(() => {});
}

function setState(link, state) {
  link.state = state;
  showState();
}

function later(link) {
  if (link.retry || link.gone) return;
  link.retry = setTimeout(() => {
    link.retry = null;
    link.open();
  }, RETRY_MS);
}

export function blocksOf(text) {
  const blocks = text.split("\n\n");
  const rest = blocks.pop();
  const data = blocks
    .map((block) => block.split("\n").filter((line) => line.startsWith("data:")).map((line) => line.slice(5).replace(/^ /, "")).join("\n"))
    .filter(Boolean);
  return { data, rest };
}

function bytesOf(text) {
  return Uint8Array.from(atob(text), (letter) => letter.charCodeAt(0));
}

export async function verified(key, signature, code) {
  if (!key || !signature) return false;
  try {
    const imported = await crypto.subtle.importKey("raw", bytesOf(key), { name: "Ed25519" }, false, ["verify"]);
    return await crypto.subtle.verify({ name: "Ed25519" }, imported, bytesOf(signature), new TextEncoder().encode(code));
  } catch {
    return false;
  }
}

async function loadLogic(link) {
  const response = await fetch(`${link.home}/api/browser/logic.js`, { headers: link.headers(), cache: "no-store" });
  if (!response.ok) throw new Error(`logic ${response.status}`);
  const code = await response.text();
  const key = await link.trusted(response.headers.get("x-logic-key"));
  if (!(await verified(key, response.headers.get("x-logic-signature"), code))) throw new Error("the served logic is not signed by the key this browser trusts");
  if (link.sandbox && link.code === code) return;
  link.code = code;
  link.sandbox?.remove();
  const frame = document.createElement("iframe");
  frame.src = "sandbox.html";
  const ready = new Promise((resolve) => frame.addEventListener("load", resolve, { once: true }));
  document.body.append(frame);
  link.sandbox = frame;
  await ready;
  const homes = await worker("homes").catch(() => []);
  frame.contentWindow.postMessage({ type: "init", code, homes }, "*");
}

function toSandbox(link, message) {
  link.sandbox?.contentWindow?.postMessage(message, "*");
}

function receive(link, raw) {
  let message;
  try {
    message = JSON.parse(raw);
  } catch {
    return;
  }
  if (message.type === "ping") {
    if (link.key === LOCAL) link.send({ type: "pong" });
  } else if (message.type === "reload") {
    worker("reload", { build: message.build }).catch(() => {});
  } else {
    toSandbox(link, { type: "server", message });
  }
}

async function hello() {
  const me = await worker("identity");
  return { ...me, version: VERSION, build: BUILD };
}

function localLink() {
  const link = { key: LOCAL, home: `http://127.0.0.1:${PORT}`, state: "disconnected", socket: null, retry: null, headers: () => ({}), trusted: async () => LOGIC_KEY };
  link.send = (message) => {
    if (link.socket?.readyState === WebSocket.OPEN) link.socket.send(JSON.stringify(message));
  };
  link.open = () => {
    const socket = link.socket;
    if (socket && (socket.readyState === WebSocket.CONNECTING || socket.readyState === WebSocket.OPEN)) return;
    let opened;
    try {
      opened = new WebSocket(SOCKET);
    } catch {
      later(link);
      return;
    }
    link.socket = opened;
    setState(link, "connecting");
    opened.onopen = async () => {
      try {
        await loadLogic(link);
        opened.send(JSON.stringify({ type: "hello", ...(await hello()) }));
        setState(link, "connected");
      } catch {
        opened.close();
      }
    };
    opened.onmessage = (event) => receive(link, event.data);
    opened.onclose = () => {
      if (link.socket !== opened) return;
      link.socket = null;
      setState(link, "disconnected");
      later(link);
    };
    opened.onerror = () => {};
  };
  return link;
}

async function listen(link, response) {
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let pending = "";
  for (;;) {
    const chunk = await reader.read();
    if (chunk.done) return;
    pending += decoder.decode(chunk.value, { stream: true }).replace(/\r\n/g, "\n");
    const { data, rest } = blocksOf(pending);
    pending = rest;
    for (const raw of data) receive(link, raw);
  }
}

function serverLink(server) {
  const link = { key: server.base, home: server.base, token: server.token, state: "disconnected", retry: null, gone: false, running: false };
  link.headers = () => ({ authorization: `Bearer ${link.token}` });
  link.trusted = (offered) => worker("pin", { base: link.home, key: offered });
  link.send = (message) => {
    fetch(`${link.home}/api/browser/reply`, { method: "POST", headers: { ...link.headers(), "content-type": "application/json" }, body: JSON.stringify(message) }).catch(() => {});
  };
  link.ask = async (message) => {
    const response = await fetch(`${link.home}/api/browser/loopback`, { method: "POST", headers: { ...link.headers(), "content-type": "application/json" }, body: JSON.stringify(message) });
    return response.ok ? response.json() : { status: 502, body: null };
  };
  link.open = async () => {
    if (link.running || link.gone) return;
    link.running = true;
    setState(link, "connecting");
    try {
      const query = new URLSearchParams(await hello());
      query.delete("deviceId");
      const response = await fetch(`${link.home}/api/browser/stream?${query}`, { headers: { ...link.headers(), accept: "text/event-stream" }, cache: "no-store" });
      if (response.status === 401) {
        link.gone = true;
        await worker("forget", { base: link.home });
        await connect();
        return;
      }
      if (!response.ok || !response.body) throw new Error(String(response.status));
      await loadLogic(link);
      setState(link, "connected");
      await listen(link, response);
    } catch {
    } finally {
      link.running = false;
      if (!link.gone) {
        setState(link, "disconnected");
        later(link);
      }
    }
  };
  return link;
}

function drop(key) {
  const link = links.get(key);
  if (!link) return;
  link.gone = true;
  clearTimeout(link.retry);
  link.socket?.close();
  link.sandbox?.remove();
  links.delete(key);
}

export async function connect() {
  const known = await worker("servers").catch(() => []);
  for (const key of [...links.keys()]) {
    if (key !== LOCAL && !known.some((server) => server.base === key && server.token === links.get(key).token)) drop(key);
  }
  if (SOCKET && !links.has(LOCAL)) links.set(LOCAL, localLink());
  for (const server of known) {
    if (!links.has(server.base)) links.set(server.base, serverLink(server));
  }
  for (const link of links.values()) link.open();
  showState();
}

function linkOf(source) {
  return [...links.values()].find((link) => link.sandbox?.contentWindow === source);
}

window.addEventListener("message", async (event) => {
  const link = linkOf(event.source);
  const message = event.data;
  if (!link || !message) return;
  if (message.type === "send") link.send(message.message);
  if (message.type !== "call" || !SANDBOX_KINDS.has(message.kind)) return;
  try {
    const value = message.kind === "loopback" ? await asked(link, message.payload) : await worker(message.kind, message.payload);
    toSandbox(link, { type: "answer", id: message.id, value });
  } catch (error) {
    toSandbox(link, { type: "answer", id: message.id, error: String(error?.message || error) });
  }
});

chrome.runtime.onMessage.addListener((message) => {
  if (message?.type === "shell-event") {
    for (const link of links.values()) toSandbox(link, { type: "event", name: message.name, args: message.args });
  }
  if (message?.type === "servers-changed") connect();
  return false;
});

connect();
