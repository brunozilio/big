import { PORT } from "../config.js";
import { attach } from "./cdp.js";
import { computer, shot, viewport } from "./computer.js";
import { captureFrame, gif } from "./frames.js";
import { connect, handle, identity, rename, send, status } from "./link.js";
import { readConsole, readNetwork } from "./logs.js";
import { candidates, formInput, javascript, pageText, putFiles, readPage } from "./page.js";
import { ToolError, blobToBase64, clip, fail, fromBase64, ok, refuse, sleep } from "./results.js";
import { answered, ask, cancel, ensure, forgetOnce } from "./sites.js";
import * as shortcuts from "./shortcuts.js";
import { context, create, groupOf, listing, loaded, newTab, owned, tabsOf } from "./tabs.js";

const seen = new Map();
const pairings = new Map();

async function signature(session) {
  const tabs = await tabsOf(await groupOf(session));
  return tabs.map((tab) => `${tab.id}:${tab.url}`).join("|");
}

async function withContext(session, result, tabId, always) {
  const now = await signature(session);
  const changed = seen.get(session) !== now;
  seen.set(session, now);
  if (!always && !changed) return result;
  result.content.push({ type: "text", text: await context(session, tabId) });
  return result;
}

async function onPage(session, input, tool) {
  const tab = await owned(session, input.tabId);
  await ensure(tab.url, tool, input.action_summary);
  await attach(tab.id).catch(() => {});
  return tab;
}

async function tabsContext({ session, input }) {
  let groupId = await groupOf(session);
  if (groupId === null) {
    if (!input.createIfEmpty) {
      return ok("No MCP tab group exists for this session yet. Call tabs_context_mcp with createIfEmpty: true to create one (it opens a new window with an empty tab).");
    }
    groupId = (await create(session)).groupId;
  }
  seen.set(session, await signature(session));
  return ok(JSON.stringify(await listing(session)));
}

async function tabsCreate({ session }) {
  const tab = await newTab(session);
  attach(tab.id).catch(() => {});
  return withContext(session, ok(`Created new tab. Tab ID: ${tab.id}`), tab.id, true);
}

async function tabsClose({ session, input }) {
  const tab = await owned(session, input.tabId);
  await chrome.tabs.remove(tab.id);
  const remaining = await tabsOf(await groupOf(session));
  const note = remaining.length === 0 ? " It was the group's last tab, so the group is gone; tabs_context_mcp with createIfEmpty: true starts a new one." : "";
  return withContext(session, ok(`Closed tab ${tab.id}.${note}`), undefined, true);
}

function normalized(url) {
  const trimmed = String(url || "").trim();
  if (!trimmed) refuse("`url` is required.");
  if (/^javascript:/i.test(trimmed)) refuse("javascript: URLs are not navigated; use javascript_tool to run code in the page.");
  return /^[a-z][a-z0-9+.-]*:/i.test(trimmed) ? trimmed : `https://${trimmed}`;
}

async function standaloneTab(session) {
  if ((await groupOf(session)) === null) await create(session);
  const [first] = await tabsOf(await groupOf(session));
  if (!first) refuse("Could not open a tab for this session. Call tabs_context_mcp with createIfEmpty: true.");
  return first;
}

async function navigate({ session, input, standalone }) {
  const history = input.url === "back" || input.url === "forward";
  const missingTab = input.tabId === undefined || input.tabId === null;
  if (missingTab && (history || !standalone)) {
    refuse(history ? 'tabId is required for url "back"/"forward".' : "navigate inside browser_batch requires an explicit tabId.");
  }
  const tab = missingTab ? await standaloneTab(session) : await owned(session, input.tabId);
  const started = Date.now();
  if (history) {
    await ensure(tab.url, "navigate");
    try {
      await (input.url === "back" ? chrome.tabs.goBack(tab.id) : chrome.tabs.goForward(tab.id));
    } catch {
      refuse(`There is no page to go ${input.url} to in tab ${tab.id}'s history.`);
    }
  } else {
    const url = normalized(input.url);
    await ensure(url, "navigate");
    await attach(tab.id).catch(() => {});
    await chrome.tabs.update(tab.id, { url });
  }
  await sleep(150);
  const now = await loaded(tab.id, started);
  const view = await viewport(tab.id).catch(() => null);
  await captureFrame(now, view, { type: "navigate", detail: now.url });
  const done = ok(`Navigated to ${now.url}${now.title ? ` ("${clip(now.title, 120)}")` : ""}`);
  const answer = await withContext(session, done, tab.id, true);
  if (missingTab) answer.content.push({ type: "text", text: JSON.stringify(await listing(session)) });
  return answer;
}

function described(input) {
  const detail = { type: clip(input.text || "", 60), key: input.text || "" }[input.action] || "";
  return { type: input.action, summary: input.action_summary, detail };
}

async function computerTool({ session, input }) {
  const tab = await onPage(session, input, "computer");
  const done = await computer(tab, input);
  const action = ["screenshot", "zoom", "wait"].includes(input.action) ? null : { ...described(input), point: done.point, from: done.from, to: done.to };
  if (input.action !== "zoom" && input.action !== "wait") await captureFrame(await chrome.tabs.get(tab.id), done.view || (await viewport(tab.id).catch(() => null)), action);
  return withContext(session, done.result, tab.id, false);
}

async function readPageTool({ session, input }) {
  const tab = await onPage(session, input, "read_page");
  return ok(await readPage(tab, input));
}

async function findTool({ session, input }) {
  if (!String(input.query || "").trim()) refuse("`query` is required: describe what to find.");
  const tab = await onPage(session, input, "find");
  return candidates(tab);
}

async function formInputTool({ session, input }) {
  const tab = await onPage(session, input, "form_input");
  const message = await formInput(tab, input.ref, input.value);
  await captureFrame(tab, await viewport(tab.id).catch(() => null), { type: "form_input", summary: input.action_summary, detail: input.ref });
  return withContext(session, ok(message), tab.id, false);
}

async function pageTextTool({ session, input }) {
  const tab = await onPage(session, input, "get_page_text");
  return ok(await pageText(tab));
}

async function javascriptTool({ session, input }) {
  if (input.action !== "javascript_exec") refuse("`action` must be 'javascript_exec'.");
  const tab = await onPage(session, input, "javascript_tool");
  const answer = await javascript(tab, input.text);
  if (answer.error) return fail(answer.error);
  return withContext(session, ok(answer.value), tab.id, false);
}

async function consoleTool({ session, input }) {
  const tab = await onPage(session, input, "read_console_messages");
  return ok(readConsole(tab, input));
}

async function networkTool({ session, input }) {
  const tab = await onPage(session, input, "read_network_requests");
  return ok(readNetwork(tab, input));
}

async function resizeTool({ session, input }) {
  const tab = await owned(session, input.tabId);
  const width = Math.round(Number(input.width));
  const height = Math.round(Number(input.height));
  if (!(width >= 200 && height >= 200)) refuse("`width` and `height` must be numbers of at least 200 pixels.");
  await chrome.windows.update(tab.windowId, { state: "normal" });
  const window = await chrome.windows.update(tab.windowId, { width, height });
  const view = await viewport(tab.id).catch(() => null);
  const inner = view ? ` The page viewport is now ${view.width}x${view.height}; take a new screenshot before using coordinates.` : "";
  return ok(`Resized window to ${window.width}x${window.height}.${inner}`);
}

async function gifTool({ input, session }) {
  const tab = await owned(session, input.tabId);
  return gif(tab, input);
}

async function asPng(data, mimeType) {
  if (mimeType === "image/png") return data;
  const bitmap = await createImageBitmap(new Blob([fromBase64(data)], { type: mimeType }));
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
  canvas.getContext("2d").drawImage(bitmap, 0, 0);
  return blobToBase64(await canvas.convertToBlob({ type: "image/png" }));
}

async function target(tab, input) {
  const hasRef = Boolean(input.ref);
  const hasPoint = Array.isArray(input.coordinate) && input.coordinate.length === 2;
  if (hasRef === hasPoint) refuse("Provide either `ref` or `coordinate`, not both and not neither.");
  if (hasRef) return { ref: input.ref };
  const view = await viewport(tab.id);
  return { x: Number(input.coordinate[0]) / view.factor, y: Number(input.coordinate[1]) / view.factor };
}

function uploaded(placed, where) {
  const names = placed.names.join(", ");
  return placed.how === "input" ? `Uploaded ${names} to ${where}.` : `Dropped ${names} on ${placed.on || "the page"} (${where}).`;
}

async function uploadImageTool({ session, input }) {
  const tab = await onPage(session, input, "upload_image");
  const kept = shot(input.imageId);
  const filename = String(input.filename || "image.png");
  const png = /\.png$/i.test(filename);
  const data = png ? await asPng(kept.data, kept.mimeType) : kept.data;
  const placed = await putFiles(tab, await target(tab, input), [{ name: filename, mimeType: png ? "image/png" : kept.mimeType, data }]);
  return withContext(session, ok(uploaded(placed, input.ref || `(${input.coordinate.join(", ")})`)), tab.id, false);
}

async function fileUploadTool({ session, input }) {
  const tab = await onPage(session, input, "file_upload");
  if (!input.ref) refuse("`ref` is required: the file input from read_page or find.");
  if (!Array.isArray(input.files) || input.files.length === 0) refuse("No files were attached to the upload.");
  const placed = await putFiles(tab, { ref: input.ref }, input.files);
  return withContext(session, ok(uploaded(placed, input.ref)), tab.id, false);
}

async function shortcutsList() {
  return ok(await shortcuts.listed());
}

async function shortcutsExecute({ session, input }) {
  const tab = await owned(session, input.tabId);
  const shortcut = await shortcuts.find(input.shortcutId);
  let opened = "";
  if (shortcut.url) {
    const url = normalized(shortcut.url);
    await ensure(url, "shortcuts_execute");
    const started = Date.now();
    await chrome.tabs.update(tab.id, { url });
    await sleep(150);
    const now = await loaded(tab.id, started);
    opened = `Opened ${now.url} in tab ${tab.id}.\n\n`;
  }
  const body = `${opened}Running shortcut /${shortcuts.command(shortcut.name)}. The user's instructions for it:\n\n${shortcut.prompt}\n\nCarry these out in tab ${tab.id} with the browser tools.`;
  return withContext(session, ok(body), tab.id, Boolean(shortcut.url));
}

const TOOLS = {
  tabs_context_mcp: tabsContext,
  tabs_create_mcp: tabsCreate,
  tabs_close_mcp: tabsClose,
  navigate,
  computer: computerTool,
  read_page: readPageTool,
  find: findTool,
  form_input: formInputTool,
  get_page_text: pageTextTool,
  javascript_tool: javascriptTool,
  read_console_messages: consoleTool,
  read_network_requests: networkTool,
  resize_window: resizeTool,
  gif_creator: gifTool,
  upload_image: uploadImageTool,
  file_upload: fileUploadTool,
  shortcuts_list: shortcutsList,
  shortcuts_execute: shortcutsExecute,
};

async function call(message) {
  const run = TOOLS[message.tool];
  if (!run) return fail(`This browser extension does not know the tool "${message.tool}". Update the extension.`);
  if (await stopped()) return fail("The user stopped the agent in the browser extension. Tell the user; they can resume it from the extension's popup.");
  const started = Date.now();
  let result;
  try {
    result = await run({ session: String(message.session || "default"), input: message.input || {}, standalone: message.standalone !== false });
  } catch (error) {
    result = fail(error instanceof ToolError ? error.message : `${message.tool} failed: ${error?.message || error}`);
  }
  await audit(message, result, Date.now() - started);
  return result;
}

async function stopped() {
  return (await chrome.storage.local.get("stopped")).stopped === true;
}

async function audit(message, result, ms) {
  const input = message.input || {};
  const tab = Number.isFinite(Number(input.tabId)) ? await chrome.tabs.get(Number(input.tabId)).catch(() => null) : null;
  let host = "";
  try {
    host = new URL(tab?.url || "").hostname;
  } catch {}
  const entry = {
    at: new Date().toISOString(),
    tool: message.tool,
    action: input.action || "",
    summary: clip(input.action_summary || "", 120),
    host,
    ok: result?.isError !== true,
    ms,
  };
  const log = (await chrome.storage.local.get("audit")).audit || [];
  log.push(entry);
  await chrome.storage.local.set({ audit: log.slice(-500) });
}

async function stopAll() {
  await chrome.storage.local.set({ stopped: true });
  forgetOnce();
  for (const target of await chrome.debugger.getTargets()) {
    if (target.attached && target.tabId !== undefined) await chrome.debugger.detach({ tabId: target.tabId }).catch(() => {});
  }
}

async function pair(id) {
  const askId = crypto.randomUUID();
  pairings.set(id, askId);
  const me = await identity();
  const answer = await ask({ kind: "pair", name: me.name }, askId);
  pairings.delete(id);
  if (answer.choice !== "connect") return;
  const name = String(answer.name || "").trim();
  if (name) await chrome.storage.local.set({ deviceName: name });
  send({ type: "paired", id, name });
}

function pairEnded(id) {
  const askId = pairings.get(id);
  if (askId) cancel(askId);
}

handle({ call, pair, pairEnded });

chrome.runtime.onMessage.addListener((message, _sender, reply) => {
  if (message?.type === "ask-answer") {
    answered(message.id, message);
    return false;
  }
  if (message?.type === "status") {
    identity().then((me) => reply({ state: status(), port: PORT, ...me }));
    return true;
  }
  if (message?.type === "rename") {
    rename(String(message.name || "").trim()).then(() => reply({ done: true }));
    return true;
  }
  if (message?.type === "stop") {
    stopAll().then(() => reply({ done: true }));
    return true;
  }
  if (message?.type === "resume") {
    chrome.storage.local.set({ stopped: false }).then(() => reply({ done: true }));
    return true;
  }
  if (message?.type === "reconnect") {
    connect();
    reply({ done: true });
  }
  return false;
});

chrome.alarms.create("link", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "link") connect();
});
chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(connect);
connect();
