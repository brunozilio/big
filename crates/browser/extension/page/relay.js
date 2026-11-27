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

  function refusal(from, to) {
    const source = parsed(from);
    const target = parsed(to);
    if (!source || !target) return "bad-url";
    if (source.protocol !== "http:" || !LOOPBACK.has(source.hostname) || source.search || source.hash) return "bad-from";
    if (target.origin !== location.origin) return "bad-to";
    if (target.protocol !== "https:" && !LOOPBACK.has(location.hostname)) return "bad-to";
    return null;
  }

  function reply(id, ok, reason) {
    const answer = { source: MARKER, type: "relay-sign-in-ready", id, ok };
    if (reason) answer.reason = reason;
    window.postMessage(answer, location.origin === "null" ? "*" : location.origin);
  }

  async function forward(data) {
    const reason = refusal(data.from, data.to);
    if (reason) return reply(data.id, false, reason);
    try {
      const answer = await chrome.runtime.sendMessage({ type: "relay-sign-in", from: String(data.from), to: String(data.to) });
      reply(data.id, answer?.ok === true, answer?.ok === true ? undefined : answer?.reason || "refused");
    } catch {
      reply(data.id, false, "unavailable");
    }
  }

  window.addEventListener("message", (event) => {
    if (event.source !== window) return;
    const data = event.data;
    if (!data || data.source !== MARKER || data.type !== "relay-sign-in") return;
    forward(data);
  });

  function mark() {
    const root = document.documentElement;
    if (!root) return false;
    root.setAttribute("data-sign-in-relay", MARKER);
    return true;
  }

  if (!mark()) {
    const observer = new MutationObserver(() => {
      if (mark()) observer.disconnect();
    });
    observer.observe(document, { childList: true });
  }
})();
