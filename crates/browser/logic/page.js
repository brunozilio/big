import { AGENT } from "./agent.js";
import { attach, onEvent, send } from "./cdp.js";
import { clip, refuse } from "./results.js";

const RESTRICTED = /^(chrome|chrome-extension|edge|about|devtools|view-source|chrome-search):/i;
const MOST_TEXT = 100000;
const MOST_JS = 100000;

function restricted(tab) {
  const url = tab.url || tab.pendingUrl || "";
  if (RESTRICTED.test(url) || url.startsWith("https://chromewebstore.google.com")) {
    refuse(`Tab ${tab.id} shows ${url || "a browser page"}, which extensions cannot read or script. Navigate it to a web page first.`);
  }
}

const worlds = new Map();

onEvent((tabId, method, params) => {
  if (method === "Runtime.executionContextsCleared") worlds.delete(tabId);
  if (method === "Runtime.executionContextDestroyed" && worlds.get(tabId) === params.executionContextId) worlds.delete(tabId);
});

async function world(tabId) {
  if (worlds.has(tabId)) return worlds.get(tabId);
  const { frameTree } = await send(tabId, "Page.getFrameTree");
  const made = await send(tabId, "Page.createIsolatedWorld", { frameId: frameTree.frame.id, worldName: "agent", grantUniveralAccess: true });
  worlds.set(tabId, made.executionContextId);
  return made.executionContextId;
}

async function evaluated(tabId, expression) {
  for (let tries = 0; ; tries += 1) {
    const contextId = await world(tabId);
    try {
      return await send(tabId, "Runtime.evaluate", { expression, contextId, returnByValue: true, awaitPromise: true });
    } catch (error) {
      worlds.delete(tabId);
      if (tries > 0) throw error;
    }
  }
}

export async function inPage(tab, name, args = []) {
  restricted(tab);
  await attach(tab.id);
  const expression = `${AGENT}\n;globalThis.__browserAgent.call(${JSON.stringify(name)}, ${JSON.stringify(args)})`;
  const answer = await evaluated(tab.id, expression);
  if (answer?.exceptionDetails) refuse(`The page script failed: ${answer.exceptionDetails.exception?.description || answer.exceptionDetails.text}`);
  const found = answer?.result?.value;
  if (found && found.error) refuse(found.error);
  return found?.value;
}

export async function readPage(tab, input) {
  const options = {
    filter: input.filter === "interactive" ? "interactive" : "all",
    depth: Number.isFinite(Number(input.depth)) ? Math.max(1, Number(input.depth)) : 15,
    refId: input.ref_id || null,
  };
  const tree = await inPage(tab, "readTree", [options]);
  const most = Number.isFinite(Number(input.max_chars)) ? Math.max(1000, Number(input.max_chars)) : 50000;
  const body = tree.lines.join("\n");
  if (!body) {
    return options.filter === "interactive"
      ? "No interactive elements found on the page. Take a screenshot to see its content, or read_page without the filter."
      : "The page has no readable elements. It may still be loading, or draw its content on a canvas; take a screenshot to see it.";
  }
  if (body.length <= most) return body;
  const cut = body.lastIndexOf("\n", most);
  const kept = body.slice(0, cut > 0 ? cut : most);
  return `${kept}\n\n[Output truncated at ${kept.length} of ${body.length} characters. Pass a larger max_chars, or use depth or ref_id to focus on part of the page.]`;
}

export async function candidates(tab) {
  return inPage(tab, "candidates", []);
}

export async function pageText(tab) {
  const found = await inPage(tab, "pageText", []);
  let body = found.text;
  if (body.length > MOST_TEXT) {
    body = `${body.slice(0, MOST_TEXT)}\n\n[Text truncated at ${MOST_TEXT} of ${found.text.length} characters. Use read_page or find to reach a specific part.]`;
  }
  return `Title: ${found.title}\nURL: ${found.url}\nSource element: <${found.source}>\n---\n${body || "(no text on this page)"}`;
}

export async function formInput(tab, ref, value) {
  if (!ref) refuse("`ref` is required: an element reference from read_page or find.");
  if (value === undefined) refuse("`value` is required.");
  return inPage(tab, "formInput", [ref, value]);
}

export async function refPoint(tab, ref) {
  return inPage(tab, "refPoint", [ref]);
}

export async function scrollTo(tab, ref) {
  return inPage(tab, "scrollTo", [ref]);
}

export async function putFiles(tab, target, files) {
  return inPage(tab, "putFiles", [target, files]);
}

export async function javascript(tab, code, contextId) {
  if (!code || !String(code).trim()) refuse("`text` is required: the JavaScript to run.");
  restricted(tab);
  const answer = await send(tab.id, "Runtime.evaluate", {
    expression: String(code),
    ...(contextId === undefined ? {} : { contextId }),
    replMode: true,
    awaitPromise: true,
    returnByValue: true,
    userGesture: true,
    includeCommandLineAPI: true,
    generatePreview: true,
  });
  if (answer.exceptionDetails) {
    const details = answer.exceptionDetails;
    const description = details.exception?.description || details.exception?.value || details.text;
    return { error: `Error: ${description}` };
  }
  return { value: clip(shown(answer.result), MOST_JS), ...structured(answer.result) };
}

function structured(result) {
  if (!result || result.type === "undefined" || !("value" in result)) return { json: null };
  const text = JSON.stringify(result.value) ?? "null";
  if (text.length > MOST_JS) return { json: null, tooLarge: true };
  return { json: JSON.parse(text) };
}

function shown(result) {
  if (!result) return "undefined";
  if (result.type === "undefined") return "undefined";
  if (result.unserializableValue) return result.unserializableValue;
  if (result.subtype === "node") return result.description || "[DOM node]";
  if ("value" in result) {
    if (typeof result.value === "string") return result.value;
    const json = JSON.stringify(result.value, null, 2);
    if (json !== undefined && !(json === "{}" && result.description && result.description !== "Object")) return json;
  }
  return result.description || String(result.value);
}
