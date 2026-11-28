import { NAME } from "../config.js";
import { attach } from "./cdp.js";
import { refuse, sleep } from "./results.js";

const KEY = "groups";
const LOAD_WAIT_MS = 30000;

async function groups() {
  const saved = await chrome.storage.session.get(KEY);
  return saved[KEY] || {};
}

async function remember(session, groupId) {
  const all = await groups();
  if (groupId === null) delete all[session];
  else all[session] = groupId;
  await chrome.storage.session.set({ [KEY]: all });
}

export async function groupOf(session) {
  const groupId = (await groups())[session];
  if (groupId === undefined) return null;
  try {
    await chrome.tabGroups.get(groupId);
    return groupId;
  } catch {
    await remember(session, null);
    return null;
  }
}

export async function sessionOfGroup(groupId) {
  const all = await groups();
  return Object.keys(all).find((session) => all[session] === groupId) || null;
}

export async function create(session) {
  const window = await chrome.windows.create({ url: "chrome://newtab/", focused: true });
  const tab = window.tabs?.[0] || (await chrome.tabs.query({ windowId: window.id }))[0];
  const groupId = await chrome.tabs.group({ tabIds: [tab.id], createProperties: { windowId: window.id } });
  await chrome.tabGroups.update(groupId, { title: NAME, color: "orange", collapsed: false });
  await remember(session, groupId);
  return { groupId, tab };
}

export async function tabsOf(groupId) {
  if (groupId === null) return [];
  const tabs = await chrome.tabs.query({ groupId });
  return tabs.sort((a, b) => a.windowId - b.windowId || a.index - b.index);
}

export async function newTab(session) {
  const groupId = await groupOf(session);
  if (groupId === null) {
    const created = await create(session);
    return created.tab;
  }
  const [first] = await tabsOf(groupId);
  const tab = await chrome.tabs.create({ url: "chrome://newtab/", windowId: first?.windowId, active: true });
  await chrome.tabs.group({ tabIds: [tab.id], groupId });
  return tab;
}

export async function owned(session, tabId) {
  if (tabId === undefined || tabId === null || !Number.isFinite(Number(tabId))) {
    refuse("`tabId` is required. Call tabs_context_mcp to get the tab IDs in this session's group.");
  }
  const id = Number(tabId);
  let tab;
  try {
    tab = await chrome.tabs.get(id);
  } catch {
    refuse(`Tab ${id} does not exist (it may have been closed). Call tabs_context_mcp for the current tab IDs.`);
  }
  const groupId = await groupOf(session);
  if (groupId === null || tab.groupId !== groupId) {
    refuse(`Tab ${id} is not in this session's tab group. Call tabs_context_mcp for valid tab IDs, or tabs_create_mcp for a new tab.`);
  }
  return tab;
}

export async function context(session, executedOn) {
  const tabs = await tabsOf(await groupOf(session));
  const lines = ["Tab Context:"];
  if (executedOn !== undefined) lines.push(`- Executed on tabId: ${executedOn}`);
  if (tabs.length === 0) {
    lines.push("- No tabs in this session's group. Use tabs_context_mcp with createIfEmpty: true to create one.");
  } else {
    lines.push("- Available tabs:");
    for (const tab of tabs) lines.push(`  • tabId ${tab.id}: "${tab.title || ""}" (${tab.url || tab.pendingUrl || ""})`);
  }
  return lines.join("\n");
}

export async function listing(session) {
  const groupId = await groupOf(session);
  const tabs = await tabsOf(groupId);
  return {
    availableTabs: tabs.map((tab) => ({ tabId: tab.id, title: tab.title || "", url: tab.url || tab.pendingUrl || "" })),
    tabGroupId: groupId,
  };
}

export async function loaded(tabId, startedAt = Date.now()) {
  while (Date.now() - startedAt < LOAD_WAIT_MS) {
    const tab = await chrome.tabs.get(tabId).catch(() => null);
    if (!tab) refuse(`Tab ${tabId} was closed while loading.`);
    if (tab.status === "complete") return tab;
    await sleep(150);
  }
  return chrome.tabs.get(tabId);
}

export async function activate(tab) {
  if (!tab.active) await chrome.tabs.update(tab.id, { active: true });
  return tab;
}

chrome.tabs.onCreated.addListener(async (tab) => {
  if (tab.openerTabId === undefined) return;
  const opener = await chrome.tabs.get(tab.openerTabId).catch(() => null);
  if (!opener || opener.groupId === -1) return;
  if (!(await sessionOfGroup(opener.groupId))) return;
  await chrome.tabs.group({ tabIds: [tab.id], groupId: opener.groupId }).catch(() => {});
  attach(tab.id).catch(() => {});
});
