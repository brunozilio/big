import { attach, onEvent, send } from "./cdp.js";
import { refuse } from "./results.js";

const contexts = new Map();

onEvent((tabId, method, params) => {
  if (method === "Runtime.executionContextCreated") {
    const { id, auxData } = params.context || {};
    if (!auxData?.isDefault || !auxData.frameId) return;
    const frames = contexts.get(tabId) || new Map();
    frames.set(auxData.frameId, id);
    contexts.set(tabId, frames);
  } else if (method === "Runtime.executionContextDestroyed") {
    const frames = contexts.get(tabId);
    for (const [frameId, id] of frames || []) if (id === params.executionContextId) frames.delete(frameId);
  } else if (method === "Runtime.executionContextsCleared") {
    contexts.delete(tabId);
  }
});

function flatten(tree, found = []) {
  found.push(tree.frame);
  for (const child of tree.childFrames || []) flatten(child, found);
  return found;
}

function originOf(url) {
  try {
    return new URL(url).origin;
  } catch {
    return "";
  }
}

function matches(url, { origin, urlPrefix }) {
  if (origin && originOf(url) !== origin) return false;
  if (urlPrefix && !String(url).startsWith(urlPrefix)) return false;
  return true;
}

export function frameMatch(input) {
  const frame = input?.frame;
  if (frame === undefined || frame === null) return null;
  const origin = typeof frame.origin === "string" ? frame.origin : "";
  const urlPrefix = typeof frame.urlPrefix === "string" ? frame.urlPrefix : "";
  if (!origin && !urlPrefix) refuse("`frame` needs an `origin` or a `urlPrefix` to find the frame in the page.");
  return { origin, urlPrefix };
}

export async function frameOf(tab, match) {
  await attach(tab.id).catch(() => {});
  const { frameTree } = await send(tab.id, "Page.getFrameTree");
  const found = flatten(frameTree).find((frame) => matches(frame.url, match));
  if (!found) {
    const wanted = match.urlPrefix || match.origin;
    refuse(`No frame of tab ${tab.id} has the address ${wanted}. Check the page's frames with javascript_tool first.`);
  }
  return { id: found.id, url: found.url };
}

export function contextOf(tabId, frameId) {
  const id = contexts.get(tabId)?.get(frameId);
  if (id === undefined) refuse("The frame has no script context yet. Wait for the page to load and try again.");
  return id;
}
