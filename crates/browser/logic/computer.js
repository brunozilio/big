import { evaluate, send } from "./cdp.js";
import { heldKeys, modifierMask, sequence } from "./keys.js";
import { refPoint, scrollTo } from "./page.js";
import { blobToBase64, fromBase64, image, ok, refuse, sleep } from "./results.js";
import { activate } from "./tabs.js";

const MOST_PIXELS = 1150000;
const MOST_SIDE = 1568;
const SHOT_LIFETIME_MS = 5 * 60 * 1000;
const shots = new Map();
let mac = null;

async function isMac() {
  if (mac === null) mac = (await chrome.runtime.getPlatformInfo()).os === "mac";
  return mac;
}

export async function viewport(tabId) {
  const view = await evaluate(tabId, "({ width: innerWidth, height: innerHeight, ratio: devicePixelRatio })");
  const factor = Math.min(1, Math.sqrt(MOST_PIXELS / (view.width * view.height)), MOST_SIDE / Math.max(view.width, view.height));
  return { ...view, factor, frameWidth: Math.round(view.width * factor), frameHeight: Math.round(view.height * factor) };
}

async function capture(tab) {
  await activate(tab);
  const { data } = await send(tab.id, "Page.captureScreenshot", { format: "png", fromSurface: true });
  return createImageBitmap(new Blob([fromBase64(data)], { type: "image/png" }));
}

async function encode(bitmap, crop, width, height, type = "image/jpeg") {
  const canvas = new OffscreenCanvas(Math.max(1, Math.round(width)), Math.max(1, Math.round(height)));
  const context = canvas.getContext("2d");
  context.imageSmoothingQuality = "high";
  context.drawImage(bitmap, crop.x, crop.y, crop.width, crop.height, 0, 0, canvas.width, canvas.height);
  const blob = await canvas.convertToBlob(type === "image/png" ? { type } : { type, quality: 0.82 });
  return { data: await blobToBase64(blob), width: canvas.width, height: canvas.height, mimeType: type };
}

function remember(entry) {
  const now = Date.now();
  for (const [id, kept] of shots) if (now - kept.at > SHOT_LIFETIME_MS) shots.delete(id);
  const id = `ss_${crypto.randomUUID().replaceAll("-", "").slice(0, 10)}`;
  shots.set(id, { ...entry, at: now });
  return id;
}

export function shot(id) {
  const kept = shots.get(String(id || ""));
  if (!kept) refuse(`Screenshot ID "${id}" is unknown or has expired. Take a new screenshot of what you want to upload, then upload it right away.`);
  if (Date.now() - kept.at > SHOT_LIFETIME_MS) {
    shots.delete(String(id));
    refuse(`Screenshot ID "${id}" has expired. Take a new screenshot of what you want to upload, then upload it right away.`);
  }
  return kept;
}

function scaleOf(input) {
  const scale = input.scale === undefined ? 1 : Number(input.scale);
  if (!Number.isFinite(scale) || scale < 0.1 || scale > 1) refuse("`scale` must be a number from 0.1 to 1.");
  return scale;
}

async function screenshot(tab, input) {
  const view = await viewport(tab.id);
  const bitmap = await capture(tab);
  const scale = scaleOf(input);
  const full = { x: 0, y: 0, width: bitmap.width, height: bitmap.height };
  const framed = await encode(bitmap, full, view.frameWidth, view.frameHeight);
  const id = remember({ data: framed.data, mimeType: framed.mimeType });
  const shown = scale === 1 ? framed : await encode(bitmap, full, view.frameWidth * scale, view.frameHeight * scale);
  const size = scale === 1
    ? `${framed.width}x${framed.height}, jpeg`
    : `${shown.width}x${shown.height} jpeg at scale ${scale}; coordinates are in the ${framed.width}x${framed.height} frame`;
  return { result: ok(`Successfully captured screenshot (${size}) - ID: ${id}`, image(shown.data, shown.mimeType)), view };
}

async function zoom(tab, input) {
  const region = input.region;
  if (!Array.isArray(region) || region.length !== 4 || region.some((n) => !Number.isFinite(Number(n)))) {
    refuse("`region` is required for zoom: [x0, y0, x1, y1] in screenshot coordinates.");
  }
  const [x0, y0, x1, y1] = region.map(Number);
  if (x1 <= x0 || y1 <= y0) refuse("`region` must go from the top-left (x0, y0) to the bottom-right (x1, y1) with x1 > x0 and y1 > y0.");
  const view = await viewport(tab.id);
  const bitmap = await capture(tab);
  const perFrame = bitmap.width / view.frameWidth;
  const crop = {
    x: Math.max(0, x0 * perFrame),
    y: Math.max(0, y0 * perFrame),
    width: Math.min(bitmap.width, x1 * perFrame) - Math.max(0, x0 * perFrame),
    height: Math.min(bitmap.height, y1 * perFrame) - Math.max(0, y0 * perFrame),
  };
  if (crop.width <= 0 || crop.height <= 0) refuse("`region` lies outside the visible page.");
  const grow = Math.min(MOST_SIDE / Math.max(crop.width, crop.height), Math.sqrt(MOST_PIXELS / (crop.width * crop.height)), 4);
  const scale = scaleOf(input);
  const zoomed = await encode(bitmap, crop, crop.width * grow * scale, crop.height * grow * scale);
  const id = remember({ data: zoomed.data, mimeType: zoomed.mimeType });
  return ok(
    `Successfully captured zoomed screenshot of region (${x0}, ${y0}) to (${x1}, ${y1}) (${zoomed.width}x${zoomed.height}, jpeg) - ID: ${id}`,
    image(zoomed.data, zoomed.mimeType),
  );
}

async function mouse(tabId, type, x, y, extra = {}) {
  await send(tabId, "Input.dispatchMouseEvent", { type, x, y, button: "none", modifiers: 0, ...extra });
}

async function pointFor(tab, input, view, wanted = "coordinate") {
  if (input.ref) {
    const point = await refPoint(tab, input.ref);
    return { x: point.x, y: point.y, label: `${input.ref}`, shownX: Math.round(point.x * view.factor), shownY: Math.round(point.y * view.factor) };
  }
  const coordinate = input[wanted];
  if (!Array.isArray(coordinate) || coordinate.length !== 2 || coordinate.some((n) => !Number.isFinite(Number(n)))) {
    refuse(`\`${wanted}\` ([x, y]) or \`ref\` is required for this action.`);
  }
  const [shownX, shownY] = coordinate.map(Number);
  if (shownX < 0 || shownY < 0 || shownX > view.frameWidth || shownY > view.frameHeight) {
    refuse(`(${shownX}, ${shownY}) is outside the ${view.frameWidth}x${view.frameHeight} screenshot frame. Take a new screenshot and use coordinates inside it.`);
  }
  return { x: shownX / view.factor, y: shownY / view.factor, label: null, shownX, shownY };
}

async function pressModifiers(tabId, mask, type) {
  for (const key of heldKeys(mask)) {
    await send(tabId, "Input.dispatchKeyEvent", { type, key: key.key, code: key.code, windowsVirtualKeyCode: key.keyCode, modifiers: mask });
  }
}

async function click(tab, input, view, button, count) {
  const point = await pointFor(tab, input, view);
  const modifiers = modifierMask(input.modifiers);
  await pressModifiers(tab.id, modifiers, "rawKeyDown");
  await mouse(tab.id, "mouseMoved", point.x, point.y, { modifiers });
  for (let n = 1; n <= count; n += 1) {
    await mouse(tab.id, "mousePressed", point.x, point.y, { button, clickCount: n, modifiers, buttons: button === "right" ? 2 : 1 });
    await mouse(tab.id, "mouseReleased", point.x, point.y, { button, clickCount: n, modifiers });
  }
  await pressModifiers(tab.id, modifiers, "keyUp");
  await sleep(120);
  const verbs = { 1: "Clicked", 2: "Double-clicked", 3: "Triple-clicked" };
  const verb = button === "right" ? "Right-clicked" : verbs[count];
  const where = point.label ? `${point.label} at (${point.shownX}, ${point.shownY})` : `(${point.shownX}, ${point.shownY})`;
  return { result: ok(`${verb} ${point.label ? "on " : "at "}${where}${input.modifiers ? ` with ${input.modifiers}` : ""}`), point };
}

async function typeText(tab, input) {
  if (typeof input.text !== "string" || input.text === "") refuse("`text` is required for the type action.");
  const lines = input.text.split("\n");
  for (let index = 0; index < lines.length; index += 1) {
    if (lines[index]) await send(tab.id, "Input.insertText", { text: lines[index] });
    if (index < lines.length - 1) await pressKeys(tab, "Enter", 1);
  }
  return ok(`Typed "${input.text}"`);
}

async function pressKeys(tab, text, repeat) {
  const keys = sequence(text, await isMac());
  for (let round = 0; round < repeat; round += 1) {
    for (const key of keys) {
      await pressModifiers(tab.id, key.modifiers & 7, "rawKeyDown");
      const base = { key: key.key, code: key.code, windowsVirtualKeyCode: key.keyCode, nativeVirtualKeyCode: key.keyCode, modifiers: key.modifiers };
      const down = key.text ? { ...base, type: "keyDown", text: key.text, unmodifiedText: key.text } : { ...base, type: "rawKeyDown" };
      if (key.command) down.commands = [key.command];
      await send(tab.id, "Input.dispatchKeyEvent", down);
      await send(tab.id, "Input.dispatchKeyEvent", { ...base, type: "keyUp" });
      await pressModifiers(tab.id, key.modifiers & 7, "keyUp");
    }
  }
  return keys.length * repeat;
}

async function scroll(tab, input, view) {
  const direction = input.scroll_direction;
  if (!["up", "down", "left", "right"].includes(direction)) refuse("`scroll_direction` is required: up, down, left or right.");
  const amount = Math.min(10, Math.max(1, Math.round(Number(input.scroll_amount ?? 3))));
  const point = input.coordinate || input.ref
    ? await pointFor(tab, input, view)
    : { x: view.width / 2, y: view.height / 2, shownX: Math.round(view.frameWidth / 2), shownY: Math.round(view.frameHeight / 2) };
  const step = 100 * amount;
  const deltaX = direction === "left" ? -step : direction === "right" ? step : 0;
  const deltaY = direction === "up" ? -step : direction === "down" ? step : 0;
  await mouse(tab.id, "mouseMoved", point.x, point.y);
  await mouse(tab.id, "mouseWheel", point.x, point.y, { deltaX, deltaY });
  await sleep(350);
  return { result: ok(`Scrolled ${direction} by ${amount} at (${point.shownX}, ${point.shownY})`), point };
}

async function drag(tab, input, view) {
  const from = await pointFor(tab, { coordinate: input.start_coordinate }, view);
  const to = await pointFor(tab, { coordinate: input.coordinate }, view);
  await mouse(tab.id, "mouseMoved", from.x, from.y);
  await mouse(tab.id, "mousePressed", from.x, from.y, { button: "left", clickCount: 1, buttons: 1 });
  const steps = 12;
  for (let n = 1; n <= steps; n += 1) {
    const x = from.x + ((to.x - from.x) * n) / steps;
    const y = from.y + ((to.y - from.y) * n) / steps;
    await mouse(tab.id, "mouseMoved", x, y, { button: "left", buttons: 1 });
    await sleep(16);
  }
  await mouse(tab.id, "mouseReleased", to.x, to.y, { button: "left", clickCount: 1 });
  await sleep(120);
  return { result: ok(`Dragged from (${from.shownX}, ${from.shownY}) to (${to.shownX}, ${to.shownY})`), from, to };
}

export async function computer(tab, input) {
  const action = input.action;
  if (action === "screenshot") return screenshot(tab, input);
  if (action === "zoom") return { result: await zoom(tab, input) };
  if (action === "wait") {
    const seconds = Number(input.duration);
    if (!Number.isFinite(seconds) || seconds < 0 || seconds > 10) refuse("`duration` is required for wait: 0 to 10 seconds.");
    await sleep(seconds * 1000);
    return { result: ok(`Waited for ${seconds} second${seconds === 1 ? "" : "s"}`) };
  }
  if (action === "type") return { result: await typeText(tab, input) };
  if (action === "key") {
    const repeat = input.repeat === undefined ? 1 : Math.round(Number(input.repeat));
    if (!Number.isFinite(repeat) || repeat < 1 || repeat > 100) refuse("`repeat` must be an integer from 1 to 100.");
    const count = await pressKeys(tab, input.text, repeat);
    await sleep(80);
    return { result: ok(`Pressed ${count} key${count === 1 ? "" : "s"}: ${String(input.text).trim()}${repeat > 1 ? ` (×${repeat})` : ""}`) };
  }
  if (action === "scroll_to") {
    if (!input.ref) refuse("`ref` is required for scroll_to.");
    await scrollTo(tab, input.ref);
    return { result: ok(`Scrolled ${input.ref} into view`) };
  }
  const view = await viewport(tab.id);
  if (action === "left_click") return click(tab, input, view, "left", 1);
  if (action === "double_click") return click(tab, input, view, "left", 2);
  if (action === "triple_click") return click(tab, input, view, "left", 3);
  if (action === "right_click") return click(tab, input, view, "right", 1);
  if (action === "scroll") return scroll(tab, input, view);
  if (action === "left_click_drag") return drag(tab, input, view);
  if (action === "hover") {
    const point = await pointFor(tab, input, view);
    await mouse(tab.id, "mouseMoved", point.x, point.y);
    await sleep(200);
    return { result: ok(point.label ? `Hovered over ${point.label} at (${point.shownX}, ${point.shownY})` : `Hovered at (${point.shownX}, ${point.shownY})`), point };
  }
  refuse(`Unknown action "${action}". Use one of: left_click, right_click, double_click, triple_click, type, key, screenshot, zoom, wait, scroll, scroll_to, left_click_drag, hover.`);
}
