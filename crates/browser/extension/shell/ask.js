const ANSWER_WAIT_MS = 120000;
const waiting = new Map();

export async function ask(question, id = crypto.randomUUID()) {
  const params = new URLSearchParams({ id, ...question });
  const window = await chrome.windows.create({ url: chrome.runtime.getURL(`ui/ask.html?${params}`), type: "popup", width: 460, height: 320, focused: true });
  return new Promise((resolve) => {
    const finish = (answer) => {
      if (!waiting.has(id)) return;
      waiting.delete(id);
      clearTimeout(timer);
      chrome.windows.onRemoved.removeListener(closed);
      chrome.windows.remove(window.id).catch(() => {});
      resolve(answer);
    };
    const closed = (windowId) => {
      if (windowId === window.id) finish({ choice: "decline" });
    };
    const timer = setTimeout(() => finish({ choice: "timeout" }), ANSWER_WAIT_MS);
    waiting.set(id, finish);
    chrome.windows.onRemoved.addListener(closed);
  });
}

export function answered(id, answer) {
  waiting.get(id)?.(answer);
}

export function cancel(id) {
  waiting.get(id)?.({ choice: "timeout" });
}
