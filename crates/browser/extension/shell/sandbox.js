(() => {
  const pending = new Map();
  const listeners = new Map();
  let next = 0;
  let homes = [];

  function ask(kind, payload) {
    next += 1;
    const id = next;
    parent.postMessage({ type: "call", id, kind, payload }, "*");
    return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
  }

  function event(name) {
    const kept = () => {
      if (!listeners.has(name)) listeners.set(name, new Set());
      return listeners.get(name);
    };
    return {
      addListener: (listener) => kept().add(listener),
      removeListener: (listener) => kept().delete(listener),
      hasListener: (listener) => kept().has(listener),
    };
  }

  function api(path) {
    return new Proxy(function () {}, {
      get(_target, part) {
        if (typeof part !== "string" || part === "then") return undefined;
        if (/^on[A-Z]/.test(part)) return event(`${path}.${part}`);
        return api(`${path}.${part}`);
      },
      apply(_target, _self, args) {
        return ask("chrome", { api: path, args });
      },
    });
  }

  globalThis.chrome = new Proxy({}, { get: (_target, name) => (typeof name === "string" ? api(name) : undefined) });
  globalThis.host = {
    send: (message) => parent.postMessage({ type: "send", message }, "*"),
    ask: (question, id) => ask("ask", { question, id }),
    cancel: (id) => ask("cancel", { id }),
    identity: () => ask("identity", {}),
    loopback: (message) => ask("loopback", message),
    homes: () => homes,
  };

  async function serve(message) {
    const logic = globalThis.__logic;
    if (!logic) return;
    if (message.type === "call") {
      const result = await logic.call(message);
      globalThis.host.send({ type: "result", id: message.id, result });
    } else if (message.type === "pair") {
      logic.pair(message.id);
    } else if (message.type === "pair_ended") {
      logic.pairEnded(message.id);
    } else if (message.type === "watch") {
      logic.watch(message);
    }
  }

  window.addEventListener("message", (incoming) => {
    if (incoming.source !== parent) return;
    const data = incoming.data || {};
    if (data.type === "answer") {
      const waiting = pending.get(data.id);
      pending.delete(data.id);
      if (!waiting) return;
      if (data.error !== undefined) waiting.reject(new Error(data.error));
      else waiting.resolve(data.value);
    } else if (data.type === "event") {
      for (const listener of listeners.get(data.name) || []) {
        try {
          listener(...(data.args || []));
        } catch {}
      }
    } else if (data.type === "init") {
      homes = Array.isArray(data.homes) ? data.homes : [];
      (0, eval)(String(data.code || ""));
    } else if (data.type === "server") {
      serve(data.message || {}).catch(() => {});
    }
  });
})();
