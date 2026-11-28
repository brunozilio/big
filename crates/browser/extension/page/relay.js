(() => {
  if (globalThis.__signInRelay) return;
  globalThis.__signInRelay = true;

  const MARKER = "{{NAME}}:relay";
  const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);

  function parsed(value) {
    try {
      return new URL(String(value));
    } catch {
      return null;
    }
  }

  function refusal(from, to, url) {
    const source = parsed(from);
    const target = parsed(to);
    const opening = parsed(url);
    if (!source || !target || !opening || opening.protocol !== "https:") return "bad-url";
    if (source.protocol !== "http:" || !LOOPBACK.has(source.hostname) || source.search || source.hash) return "bad-from";
    if (target.origin !== location.origin) return "bad-to";
    if (target.protocol !== "https:" && !LOOPBACK.has(location.hostname)) return "bad-to";
    return null;
  }

  function reply(id, ok, reason, opened) {
    const answer = { source: MARKER, type: "relay-sign-in-ready", id, ok, opened: opened === true };
    if (reason) answer.reason = reason;
    window.postMessage(answer, location.origin === "null" ? "*" : location.origin);
  }

  async function forward(data) {
    const reason = refusal(data.from, data.to, data.url);
    if (reason) return reply(data.id, false, reason);
    try {
      const answer = await chrome.runtime.sendMessage({ type: "relay-sign-in", from: String(data.from), to: String(data.to), url: String(data.url) });
      reply(data.id, answer?.ok === true, answer?.ok === true ? undefined : answer?.reason || "refused", answer?.opened === true);
    } catch {
      reply(data.id, false, "unavailable");
    }
  }

  async function ask(data, type) {
    const answer = { source: MARKER, type: `${data.type}-done`, id: data.id, ok: false };
    try {
      const told = await chrome.runtime.sendMessage({ type, server: String(data.server || ""), code: String(data.code || "") });
      answer.ok = told?.ok === true;
      if (!answer.ok) answer.reason = told?.reason || "refused";
    } catch {
      answer.reason = "unavailable";
    }
    window.postMessage(answer, location.origin === "null" ? "*" : location.origin);
  }

  window.addEventListener("message", (event) => {
    if (event.source !== window) return;
    const data = event.data;
    if (!data || data.source !== MARKER) return;
    if (data.type === "relay-sign-in") forward(data);
    if (data.type === "browser-connect") ask(data, "browser-connect");
    if (data.type === "browser-reload") ask(data, "browser-reload");
  });

  function mark() {
    const root = document.documentElement;
    if (!root) return false;
    root.setAttribute("data-sign-in-relay", MARKER);
    root.setAttribute("data-browser-extension", "{{BUILD}}");
    return true;
  }

  if (!mark()) {
    const observer = new MutationObserver(() => {
      if (mark()) observer.disconnect();
    });
    observer.observe(document, { childList: true });
  }
})();
