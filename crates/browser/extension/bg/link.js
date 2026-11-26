import { NAME, SOCKET, VERSION } from "../config.js";

const RETRY_MS = 3000;

let socket = null;
let retry = null;
let state = "disconnected";
let onCall = async () => null;
let onPair = () => {};
let onPairEnded = () => {};
let onWatch = () => {};

export function handle(handlers) {
  onCall = handlers.call;
  onPair = handlers.pair;
  onPairEnded = handlers.pairEnded;
  onWatch = handlers.watch || onWatch;
}

export function status() {
  return state;
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

function setState(next) {
  state = next;
  const colors = { connected: "#1a7f37", connecting: "#bf8700", disconnected: "#cf222e" };
  chrome.action.setBadgeText({ text: next === "connected" ? "" : "!" }).catch(() => {});
  chrome.action.setBadgeBackgroundColor({ color: colors[next] }).catch(() => {});
  chrome.action.setTitle({ title: `${NAME}: ${next}` }).catch(() => {});
}

export function connect() {
  if (socket && (socket.readyState === WebSocket.CONNECTING || socket.readyState === WebSocket.OPEN)) return;
  let opened;
  try {
    opened = new WebSocket(SOCKET);
  } catch {
    later();
    return;
  }
  socket = opened;
  setState("connecting");
  opened.onopen = async () => {
    const me = await identity();
    opened.send(JSON.stringify({ type: "hello", ...me, version: VERSION }));
    setState("connected");
  };
  opened.onmessage = (event) => receive(event.data);
  opened.onclose = () => {
    if (socket !== opened) return;
    socket = null;
    setState("disconnected");
    later();
  };
  opened.onerror = () => {};
}

function later() {
  if (retry) return;
  retry = setTimeout(() => {
    retry = null;
    connect();
  }, RETRY_MS);
}

export function send(message) {
  if (socket && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message));
}

async function receive(raw) {
  let message;
  try {
    message = JSON.parse(raw);
  } catch {
    return;
  }
  if (message.type === "ping") {
    send({ type: "pong" });
  } else if (message.type === "pair") {
    onPair(message.id);
  } else if (message.type === "watch") {
    onWatch(message);
  } else if (message.type === "pair_ended") {
    onPairEnded(message.id);
  } else if (message.type === "call") {
    const result = await onCall(message);
    send({ type: "result", id: message.id, result });
  }
}
