const element = (id) => document.getElementById(id);

function item(label, action, onClick) {
  const row = document.createElement("li");
  const text = document.createElement("span");
  text.textContent = label;
  text.title = label;
  const button = document.createElement("button");
  button.textContent = action;
  button.className = "danger";
  button.addEventListener("click", onClick);
  row.append(text, button);
  return row;
}

async function showStatus() {
  const status = await chrome.runtime.sendMessage({ type: "status" });
  element("dot").className = `dot ${status.state}`;
  const words = { connected: "Connected to the app", connecting: "Connecting…", disconnected: "Not connected. Is the app running?" };
  element("state").textContent = `${words[status.state] || status.state} (port ${status.port})`;
  element("name").value = status.name;
  element("device").textContent = `Device ${status.deviceId} · ${status.platform}`;
}

async function showSites() {
  const saved = await chrome.storage.local.get(["siteMode", "sites"]);
  element("mode").value = saved.siteMode || "ask";
  const sites = saved.sites || {};
  const list = element("sites");
  list.replaceChildren();
  for (const host of Object.keys(sites).sort()) {
    list.append(item(`${sites[host] === "allow" ? "Allowed" : "Blocked"}: ${host}`, "Forget", async () => {
      const current = (await chrome.storage.local.get("sites")).sites || {};
      delete current[host];
      await chrome.storage.local.set({ sites: current });
      showSites();
    }));
  }
}

async function showShortcuts() {
  const shortcuts = (await chrome.storage.local.get("shortcuts")).shortcuts || [];
  const list = element("shortcuts");
  list.replaceChildren();
  for (const shortcut of shortcuts) {
    list.append(item(`/${shortcut.name}${shortcut.url ? ` · ${shortcut.url}` : ""}`, "Delete", async () => {
      const current = (await chrome.storage.local.get("shortcuts")).shortcuts || [];
      await chrome.storage.local.set({ shortcuts: current.filter((kept) => kept.id !== shortcut.id) });
      showShortcuts();
    }));
  }
}

element("saveName").addEventListener("click", async () => {
  await chrome.runtime.sendMessage({ type: "rename", name: element("name").value });
  showStatus();
});

element("mode").addEventListener("change", async () => {
  await chrome.storage.local.set({ siteMode: element("mode").value });
});

element("addShortcut").addEventListener("click", async () => {
  const name = element("shortcutName").value.trim().replace(/^\/+/, "");
  const prompt = element("shortcutPrompt").value.trim();
  if (!name || !prompt) return;
  const current = (await chrome.storage.local.get("shortcuts")).shortcuts || [];
  current.push({ id: crypto.randomUUID().slice(0, 8), name, url: element("shortcutUrl").value.trim(), prompt });
  await chrome.storage.local.set({ shortcuts: current });
  for (const id of ["shortcutName", "shortcutUrl", "shortcutPrompt"]) element(id).value = "";
  showShortcuts();
});

async function showRun() {
  const { stopped, audit } = await chrome.storage.local.get(["stopped", "audit"]);
  element("stop").hidden = stopped === true;
  element("resume").hidden = stopped !== true;
  const list = element("audit");
  list.replaceChildren();
  for (const entry of (audit || []).slice(-8).reverse()) {
    const row = document.createElement("li");
    const text = document.createElement("span");
    const what = [entry.tool, entry.action, entry.summary].filter(Boolean).join(" · ");
    text.textContent = `${entry.ok ? "✓" : "✗"} ${entry.at.slice(11, 19)} ${what}${entry.host ? ` @ ${entry.host}` : ""}`;
    text.title = text.textContent;
    row.append(text);
    list.append(row);
  }
}

element("stop").addEventListener("click", async () => {
  await chrome.runtime.sendMessage({ type: "stop" });
  showRun();
});

element("resume").addEventListener("click", async () => {
  await chrome.runtime.sendMessage({ type: "resume" });
  showRun();
});

element("clearAudit").addEventListener("click", async () => {
  await chrome.storage.local.set({ audit: [] });
  showRun();
});

chrome.runtime.sendMessage({ type: "reconnect" });
showRun();
setInterval(showRun, 2000);
showStatus();
showSites();
showShortcuts();
setInterval(showStatus, 2000);
