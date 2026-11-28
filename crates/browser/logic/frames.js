import { NAME } from "../config.js";
import { send } from "./cdp.js";
import { viewport } from "./computer.js";
import { putFiles } from "./page.js";
import { blobToBase64, fromBase64, ok, refuse } from "./results.js";

const MOST_FRAMES = 300;
const FRAME_WIDTH = 960;
const OUT_WIDTH = 800;
const recordings = new Map();

function recordingOf(groupId) {
  if (!recordings.has(groupId)) recordings.set(groupId, { on: false, frames: [] });
  return recordings.get(groupId);
}

export function isRecording(groupId) {
  return recordings.get(groupId)?.on === true;
}

export async function captureFrame(tab, view, action) {
  const recording = recordings.get(tab.groupId);
  if (!recording?.on) return;
  try {
    const { data } = await send(tab.id, "Page.captureScreenshot", { format: "jpeg", quality: 70, fromSurface: true });
    const bitmap = await createImageBitmap(new Blob([fromBase64(data)], { type: "image/jpeg" }));
    const width = Math.min(FRAME_WIDTH, bitmap.width);
    const height = Math.round((bitmap.height * width) / bitmap.width);
    const canvas = new OffscreenCanvas(width, height);
    canvas.getContext("2d").drawImage(bitmap, 0, 0, width, height);
    const blob = await canvas.convertToBlob({ type: "image/jpeg", quality: 0.8 });
    const relative = (point) => (point && view ? { x: point.x / view.width, y: point.y / view.height } : null);
    recording.frames.push({
      jpeg: await blobToBase64(blob),
      at: Date.now(),
      action: action ? { ...action, point: relative(action.point), from: relative(action.from), to: relative(action.to) } : null,
    });
    if (recording.frames.length > MOST_FRAMES) recording.frames.shift();
  } catch {}
}

function roundRect(context, x, y, width, height, radius) {
  context.beginPath();
  context.moveTo(x + radius, y);
  context.arcTo(x + width, y, x + width, y + height, radius);
  context.arcTo(x + width, y + height, x, y + height, radius);
  context.arcTo(x, y + height, x, y, radius);
  context.arcTo(x, y, x + width, y, radius);
  context.closePath();
}

function arrow(context, from, to) {
  context.strokeStyle = "#e5322d";
  context.fillStyle = "#e5322d";
  context.lineWidth = 4;
  context.beginPath();
  context.moveTo(from.x, from.y);
  context.lineTo(to.x, to.y);
  context.stroke();
  const angle = Math.atan2(to.y - from.y, to.x - from.x);
  context.beginPath();
  context.moveTo(to.x, to.y);
  context.lineTo(to.x - 16 * Math.cos(angle - 0.45), to.y - 16 * Math.sin(angle - 0.45));
  context.lineTo(to.x - 16 * Math.cos(angle + 0.45), to.y - 16 * Math.sin(angle + 0.45));
  context.closePath();
  context.fill();
}

function label(context, body, width) {
  context.font = "600 15px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
  const text = body.length > 80 ? `${body.slice(0, 79)}…` : body;
  const measured = Math.min(context.measureText(text).width, width - 48);
  const boxWidth = measured + 28;
  const x = (width - boxWidth) / 2;
  context.fillStyle = "rgba(0, 0, 0, 0.82)";
  roundRect(context, x, 14, boxWidth, 32, 8);
  context.fill();
  context.fillStyle = "#ffffff";
  context.textBaseline = "middle";
  context.fillText(text, x + 14, 30, width - 48);
}

function watermark(context, width, height) {
  context.font = "600 13px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
  const boxWidth = context.measureText(NAME).width + 34;
  const x = width - boxWidth - 12;
  const y = height - 40;
  context.fillStyle = "rgba(255, 255, 255, 0.85)";
  roundRect(context, x, y, boxWidth, 24, 12);
  context.fill();
  context.fillStyle = "#d97706";
  context.beginPath();
  context.arc(x + 13, y + 12, 5, 0, Math.PI * 2);
  context.fill();
  context.fillStyle = "#1f2328";
  context.textBaseline = "middle";
  context.fillText(NAME, x + 24, y + 12.5);
}

function describe(action) {
  if (action.summary) return action.summary;
  const words = {
    left_click: "Click", right_click: "Right-click", double_click: "Double-click", triple_click: "Triple-click",
    type: "Type", key: "Press", scroll: "Scroll", left_click_drag: "Drag", hover: "Hover", navigate: "Navigate", form_input: "Fill in",
  };
  const verb = words[action.type] || action.type;
  return action.detail ? `${verb}: ${action.detail}` : verb;
}

async function render(groupId, options) {
  const recording = recordings.get(groupId);
  const frames = recording?.frames || [];
  const show = (key) => options?.[key] !== false;
  const rendered = [];
  for (let index = 0; index < frames.length; index += 1) {
    const frame = frames[index];
    const bitmap = await createImageBitmap(new Blob([fromBase64(frame.jpeg)], { type: "image/jpeg" }));
    const width = Math.min(OUT_WIDTH, bitmap.width);
    const height = Math.round((bitmap.height * width) / bitmap.width);
    const canvas = new OffscreenCanvas(width, height);
    const context = canvas.getContext("2d");
    context.drawImage(bitmap, 0, 0, width, height);
    const action = frame.action;
    const at = (point) => ({ x: point.x * width, y: point.y * height });
    if (action?.point && show("showClickIndicators") && action.type !== "hover" && action.type !== "scroll") {
      const point = at(action.point);
      context.fillStyle = "rgba(255, 140, 0, 0.35)";
      context.strokeStyle = "#ff8c00";
      context.lineWidth = 4;
      context.beginPath();
      context.arc(point.x, point.y, 18, 0, Math.PI * 2);
      context.fill();
      context.stroke();
    }
    if (action?.from && action?.to && show("showDragPaths")) arrow(context, at(action.from), at(action.to));
    if (action && show("showActionLabels")) label(context, describe(action), width);
    if (show("showProgressBar")) {
      context.fillStyle = "rgba(0, 0, 0, 0.15)";
      context.fillRect(0, height - 6, width, 6);
      context.fillStyle = "#ff8c00";
      context.fillRect(0, height - 6, (width * (index + 1)) / frames.length, 6);
    }
    if (show("showWatermark")) watermark(context, width, height);
    const blob = await canvas.convertToBlob({ type: "image/png" });
    const following = frames[index + 1];
    const delay = following ? Math.min(2500, Math.max(400, following.at - frame.at)) : 2000;
    rendered.push({ png: await blobToBase64(blob), delay });
  }
  return { frames: rendered };
}

async function deliver(tab, input) {
  if (input.download) {
    const id = await chrome.downloads.download({ url: `data:image/gif;base64,${input.data}`, filename: input.filename, saveAs: false, conflictAction: "uniquify" });
    return ok(`Downloaded in the browser as ${input.filename} (download ${id}).`);
  }
  const [x, y] = input.coordinate.map(Number);
  const view = await viewport(tab.id);
  const dropped = await putFiles(tab, { x: x / view.factor, y: y / view.factor }, [{ name: input.filename, mimeType: "image/gif", data: input.data }]);
  return ok(`Dropped ${input.filename} on ${dropped.on || "the page"} at (${x}, ${y}).`);
}

export async function gif(tab, input) {
  const groupId = tab.groupId;
  const recording = recordingOf(groupId);
  if (input.action === "start_recording") {
    recording.on = true;
    return ok(`Started recording browser actions in this tab group${recording.frames.length ? ` (keeping ${recording.frames.length} earlier frames; use clear to drop them)` : ""}. Take a screenshot now to capture the initial state as the first frame.`);
  }
  if (input.action === "stop_recording") {
    recording.on = false;
    return ok(`Stopped recording. ${recording.frames.length} frame${recording.frames.length === 1 ? "" : "s"} captured. Export them with gif_creator export (download: true), or clear to discard them.`);
  }
  if (input.action === "clear") {
    const count = recording.frames.length;
    recording.frames = [];
    recording.on = false;
    return ok(`Cleared ${count} frame${count === 1 ? "" : "s"} and stopped recording.`);
  }
  if (input.action === "render") return render(groupId, input.options);
  if (input.action === "deliver") return deliver(tab, input);
  if (input.action === "export") refuse("Export runs through the app; call gif_creator with action export.");
  refuse("`action` must be start_recording, stop_recording, export or clear.");
}
