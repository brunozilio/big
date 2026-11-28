import { BUILD, NAME, SOCKET, VERSION } from "../config.js";

const RETRY_MS = 3000;
const SERVERS_KEY = "servers";
const LOCAL = "local";

const links = new Map();
let handlers = { call: async () => null, pair: () => {}, pairEnded: () => {}, watch: () => {} };

export function handle(given) {
  handlers = { ...handlers, ...given };
}

export function status() {
  return [...links.values()].some((link) => link.state === "connected") ? "connected" : "disconnected";
}

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

function showState() {
  const next = status();
  const anyTrying = [...links.values()].some((link) => link.state === "connecting");
  const shown = next === "connected" ? next : anyTrying ? "connecting" : next;
  const colors = { connected: "#1a7f37", connecting: "#bf8700", disconnected: "#cf222e" };
  chrome.action.setBadgeText({ text: shown === "connected" ? "" : "!" }).catch(() => {});
  chrome.action.setBadgeBackgroundColor({ color: colors[shown] }).catch(() => {});
  chrome.action.setTitle({ title: `${NAME}: ${shown}` }).catch(() => {});
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

function localLink() {
  const link = { key: LOCAL, homes: [], state: "disconnected", socket: null, retry: null };
  if (SOCKET) {
    const port = new URL(SOCKET).port;
    link.homes = [`http://127.0.0.1:${port}`, `http://localhost:${port}`];
  }
  link.send = (message) => {
    if (link.socket && link.socket.readyState === WebSocket.OPEN) link.socket.send(JSON.stringify(message));
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
      const me = await identity();
      opened.send(JSON.stringify({ type: "hello", ...me, version: VERSION, build: BUILD }));
      setState(link, "connected");
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

export function blocksOf(text) {
  const blocks = text.split("\n\n");
  const rest = blocks.pop();
  const data = blocks
    .map((block) => block.split("\n").filter((line) => line.startsWith("data:")).map((line) => line.slice(5).replace(/^ /, "")).join("\n"))
    .filter(Boolean);
  return { data, rest };
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
  const link = { key: server.base, base: server.base, token: server.token, homes: [new URL(server.base).origin], state: "disconnected", retry: null, gone: false, running: false };
  const headers = () => ({ authorization: `Bearer ${link.token}` });
  link.send = (message) => {
    fetch(`${link.base}/api/browser/reply`, { method: "POST", headers: { ...headers(), "content-type": "application/json" }, body: JSON.stringify(message) }).catch(() => {});
  };
  link.ask = async (message) => {
    const response = await fetch(`${link.base}/api/browser/loopback`, { method: "POST", headers: { ...headers(), "content-type": "application/json" }, body: JSON.stringify(message) });
    return response.ok ? response.json() : { status: 502, body: null };
  };
  link.open = async () => {
    if (link.running || link.gone) return;
    link.running = true;
    setState(link, "connecting");
    try {
      const me = await identity();
      const query = new URLSearchParams({ name: me.name, platform: me.platform, version: VERSION, build: BUILD });
      const response = await fetch(`${link.base}/api/browser/stream?${query}`, { headers: { ...headers(), accept: "text/event-stream" }, cache: "no-store" });
      if (response.status === 401) {
        await forgetServer(link.base);
        return;
      }
      if (!response.ok || !response.body) throw new Error(String(response.status));
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

async function servers() {
  const saved = await chrome.storage.local.get(SERVERS_KEY);
  return Array.isArray(saved[SERVERS_KEY]) ? saved[SERVERS_KEY] : [];
}

export async function rememberServer(base, token) {
  const kept = (await servers()).filter((server) => server.base !== base);
  await chrome.storage.local.set({ [SERVERS_KEY]: [...kept, { base, token }] });
  const old = links.get(base);
  if (old) {
    old.gone = true;
    links.delete(base);
  }
  await connect();
}

async function forgetServer(base) {
  await chrome.storage.local.set({ [SERVERS_KEY]: (await servers()).filter((server) => server.base !== base) });
  const link = links.get(base);
  if (link) {
    link.gone = true;
    links.delete(base);
  }
  showState();
}

export async function connect() {
  if (SOCKET && !links.has(LOCAL)) links.set(LOCAL, localLink());
  for (const server of await servers()) {
    if (!links.has(server.base)) links.set(server.base, serverLink(server));
  }
  for (const link of links.values()) link.open();
}

export function homes() {
  return [...links.values()].flatMap((link) => link.homes);
}

async function receive(link, raw) {
  let message;
  try {
    message = JSON.parse(raw);
  } catch {
    return;
  }
  if (message.type === "ping") {
    await chrome.runtime.getPlatformInfo();
    if (link.key === LOCAL) link.send({ type: "pong" });
  } else if (message.type === "reload") {
    await reload(message.build);
  } else if (message.type === "pair") {
    handlers.pair(message.id, link);
  } else if (message.type === "watch") {
    handlers.watch(message, link);
  } else if (message.type === "pair_ended") {
    handlers.pairEnded(message.id);
  } else if (message.type === "call") {
    const result = await handlers.call(message);
    link.send({ type: "result", id: message.id, result });
  }
}

async function reload(build) {
  if (!build || build === BUILD) return;
  const saved = await chrome.storage.local.get("reloadedFor");
  if (saved.reloadedFor === build) return;
  await chrome.storage.local.set({ reloadedFor: build });
  chrome.runtime.reload();
}
