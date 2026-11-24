import { clip, refuse } from "./results.js";

export async function all() {
  const saved = await chrome.storage.local.get("shortcuts");
  return Array.isArray(saved.shortcuts) ? saved.shortcuts : [];
}

export async function save(shortcuts) {
  await chrome.storage.local.set({ shortcuts });
}

export function command(name) {
  return String(name || "").trim().toLowerCase().replace(/^\/+/, "").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

export async function listed() {
  const shortcuts = await all();
  if (shortcuts.length === 0) {
    return "The user has no saved shortcuts. They can add one in the browser extension's popup (a name, an optional start URL and the instructions).";
  }
  const lines = shortcuts.map((shortcut) => {
    const start = shortcut.url ? `\n  Starts at: ${shortcut.url}` : "";
    return `- id: ${shortcut.id} | /${command(shortcut.name)}${start}\n  Instructions: ${clip(String(shortcut.prompt || "").replace(/\s+/g, " "), 200)}`;
  });
  return `Saved shortcuts (${shortcuts.length}):\n\n${lines.join("\n")}`;
}

export async function find(idOrName) {
  const wanted = String(idOrName || "").trim();
  if (!wanted) refuse("`shortcutId` is required. Call shortcuts_list for the saved shortcuts.");
  const shortcuts = await all();
  const found = shortcuts.find((shortcut) => shortcut.id === wanted) || shortcuts.find((shortcut) => command(shortcut.name) === command(wanted));
  if (!found) refuse(`No saved shortcut matches "${wanted}". Call shortcuts_list for the saved shortcuts.`);
  return found;
}
