import { BUILD, NAME } from "../config.js";
import { answered, ask, cancel } from "./ask.js";
import { forwardEvents, invoke } from "./bridge.js";
import { forgetServer, homes, identity, servers } from "./identity.js";
import { relay, watchRelays } from "./relay.js";
import { connectServer, reloadFor } from "./servers.js";

const OFFSCREEN = "shell/offscreen.html";
const COLORS = { connected: "#1a7f37", connecting: "#bf8700", disconnected: "#cf222e" };
let opening = null;

async function ensureOffscreen() {
  if (await chrome.offscreen.hasDocument().catch(() => false)) return;
  if (!opening) {
    opening = chrome.offscreen
      .createDocument({ url: OFFSCREEN, reasons: ["IFRAME_SCRIPTING"], justification: `Runs the ${NAME} browser tools served by the app, in a sandbox.` })
      .catch(() => {})
      .finally(() => {
        opening = null;
      });
  }
  await opening;
}

function fromPage(sender, path) {
  return sender?.id === chrome.runtime.id && String(sender.url || "").startsWith(chrome.runtime.getURL(path));
}

function badge(state) {
  chrome.action.setBadgeText({ text: state === "connected" ? "" : "!" }).catch(() => {});
  chrome.action.setBadgeBackgroundColor({ color: COLORS[state] || COLORS.disconnected }).catch(() => {});
  chrome.action.setTitle({ title: `${NAME}: ${state}` }).catch(() => {});
}

async function reload(build) {
  if (!build || build === BUILD) return;
  const saved = await chrome.storage.local.get("reloadedFor");
  if (saved.reloadedFor === build) return;
  await chrome.storage.local.set({ reloadedFor: build });
  chrome.runtime.reload();
}

const FROM_OFFSCREEN = {
  chrome: (message) => invoke(message.api, message.args),
  ask: (message) => ask(message.question, message.id),
  cancel: async (message) => cancel(message.id),
  identity: () => identity(),
  servers: () => servers(),
  homes: () => homes(),
  forget: (message) => forgetServer(message.base),
  badge: async (message) => badge(message.state),
  reload: (message) => reload(message.build),
};

const FROM_PAGES = { "relay-sign-in": relay, "browser-connect": connectServer, "browser-reload": reloadFor };

function answer(work, sendResponse) {
  Promise.resolve()
    .then(work)
    .then(
      (value) => sendResponse({ value: value === undefined ? null : value }),
      (error) => sendResponse({ error: String(error?.message || error) }),
    );
  return true;
}

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message?.type === "ask-answer" && fromPage(sender, "ui/")) answered(message.id, message);
  if (fromPage(sender, OFFSCREEN)) {
    const work = FROM_OFFSCREEN[message?.type];
    return work ? answer(() => work(message), sendResponse) : false;
  }
  const work = FROM_PAGES[message?.type];
  if (!work) return false;
  work(message, sender).then(sendResponse, () => sendResponse({ ok: false, reason: "failed" }));
  return true;
});

forwardEvents((name, args) => {
  chrome.runtime.sendMessage({ type: "shell-event", name, args }).catch(() => {});
});
watchRelays();

chrome.alarms.create("shell", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "shell") ensureOffscreen();
});
chrome.runtime.onStartup.addListener(ensureOffscreen);
chrome.runtime.onInstalled.addListener(ensureOffscreen);
ensureOffscreen();
