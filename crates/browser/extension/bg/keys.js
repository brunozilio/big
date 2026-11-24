import { refuse } from "./results.js";

const MODIFIERS = {
  alt: 1,
  option: 1,
  opt: 1,
  ctrl: 2,
  control: 2,
  cmd: 4,
  command: 4,
  meta: 4,
  super: 4,
  win: 4,
  windows: 4,
  shift: 8,
};

const MODIFIER_KEYS = {
  1: { key: "Alt", code: "AltLeft", keyCode: 18 },
  2: { key: "Control", code: "ControlLeft", keyCode: 17 },
  4: { key: "Meta", code: "MetaLeft", keyCode: 91 },
  8: { key: "Shift", code: "ShiftLeft", keyCode: 16 },
};

const NAMED = {
  enter: { key: "Enter", code: "Enter", keyCode: 13, text: "\r" },
  return: { key: "Enter", code: "Enter", keyCode: 13, text: "\r" },
  tab: { key: "Tab", code: "Tab", keyCode: 9 },
  escape: { key: "Escape", code: "Escape", keyCode: 27 },
  esc: { key: "Escape", code: "Escape", keyCode: 27 },
  backspace: { key: "Backspace", code: "Backspace", keyCode: 8 },
  delete: { key: "Delete", code: "Delete", keyCode: 46 },
  del: { key: "Delete", code: "Delete", keyCode: 46 },
  insert: { key: "Insert", code: "Insert", keyCode: 45 },
  space: { key: " ", code: "Space", keyCode: 32, text: " " },
  arrowup: { key: "ArrowUp", code: "ArrowUp", keyCode: 38 },
  up: { key: "ArrowUp", code: "ArrowUp", keyCode: 38 },
  arrowdown: { key: "ArrowDown", code: "ArrowDown", keyCode: 40 },
  down: { key: "ArrowDown", code: "ArrowDown", keyCode: 40 },
  arrowleft: { key: "ArrowLeft", code: "ArrowLeft", keyCode: 37 },
  left: { key: "ArrowLeft", code: "ArrowLeft", keyCode: 37 },
  arrowright: { key: "ArrowRight", code: "ArrowRight", keyCode: 39 },
  right: { key: "ArrowRight", code: "ArrowRight", keyCode: 39 },
  home: { key: "Home", code: "Home", keyCode: 36 },
  end: { key: "End", code: "End", keyCode: 35 },
  pageup: { key: "PageUp", code: "PageUp", keyCode: 33 },
  page_up: { key: "PageUp", code: "PageUp", keyCode: 33 },
  pagedown: { key: "PageDown", code: "PageDown", keyCode: 34 },
  page_down: { key: "PageDown", code: "PageDown", keyCode: 34 },
  capslock: { key: "CapsLock", code: "CapsLock", keyCode: 20 },
  contextmenu: { key: "ContextMenu", code: "ContextMenu", keyCode: 93 },
};

for (let n = 1; n <= 12; n += 1) NAMED[`f${n}`] = { key: `F${n}`, code: `F${n}`, keyCode: 111 + n };

const PUNCTUATION = {
  "-": ["Minus", 189, "_"],
  "=": ["Equal", 187, "+"],
  "[": ["BracketLeft", 219, "{"],
  "]": ["BracketRight", 221, "}"],
  "\\": ["Backslash", 220, "|"],
  ";": ["Semicolon", 186, ":"],
  "'": ["Quote", 222, '"'],
  ",": ["Comma", 188, "<"],
  ".": ["Period", 190, ">"],
  "/": ["Slash", 191, "?"],
  "`": ["Backquote", 192, "~"],
};

const SHIFTED_DIGITS = ")!@#$%^&*(";

const MAC_COMMANDS = {
  a: "selectAll",
  c: "copy",
  x: "cut",
  v: "paste",
  z: "undo",
  "shift+z": "redo",
  y: "redo",
};

function single(name) {
  const lower = name.toLowerCase();
  if (NAMED[lower]) return { ...NAMED[lower] };
  if (name.length !== 1) refuse(`Unknown key "${name}". Use names like Enter, Tab, Escape, Backspace, ArrowDown, PageDown, F5, or single characters.`);
  if (/[a-z]/i.test(name)) {
    const upper = name.toUpperCase();
    return { key: name, code: `Key${upper}`, keyCode: upper.charCodeAt(0), text: name, shifted: name !== name.toLowerCase() };
  }
  if (/[0-9]/.test(name)) return { key: name, code: `Digit${name}`, keyCode: name.charCodeAt(0), text: name };
  const digit = SHIFTED_DIGITS.indexOf(name);
  if (digit >= 0) return { key: name, code: `Digit${digit}`, keyCode: 48 + digit, text: name, shifted: true };
  for (const [plain, [code, keyCode, shifted]] of Object.entries(PUNCTUATION)) {
    if (name === plain) return { key: name, code, keyCode, text: name };
    if (name === shifted) return { key: name, code, keyCode, text: name, shifted: true };
  }
  return { key: name, code: "", keyCode: 0, text: name };
}

export function parse(combo, mac) {
  const parts = combo.split("+");
  const keyName = parts.pop() || "+";
  let modifiers = 0;
  for (const part of parts) {
    const bit = MODIFIERS[part.toLowerCase()];
    if (!bit) refuse(`Unknown modifier "${part}" in "${combo}". Use ctrl, shift, alt, or cmd.`);
    modifiers |= bit;
  }
  const lowerKey = keyName.toLowerCase();
  if (modifiers & 6 && ["=", "+", "-", "0", "plus", "minus"].includes(lowerKey)) {
    refuse('Page zoom shortcuts (e.g. "cmd+=", "ctrl+-", "cmd+0") are not supported. Use the computer tool\'s `zoom` action to magnify a region of the page instead.');
  }
  const key = MODIFIERS[lowerKey] ? { ...MODIFIER_KEYS[MODIFIERS[lowerKey]] } : single(keyName);
  if (key.shifted) modifiers |= 8;
  const commandKey = (modifiers & 8 ? "shift+" : "") + lowerKey;
  const command = mac && modifiers & 4 && !(modifiers & 3) ? MAC_COMMANDS[commandKey] : undefined;
  if (modifiers & 7) delete key.text;
  return { ...key, modifiers, command };
}

export function sequence(text, mac) {
  const combos = String(text || "").trim().split(/\s+/).filter(Boolean);
  if (combos.length === 0) refuse("`text` is required for the key action: the key(s) to press, e.g. \"Enter\" or \"cmd+a\".");
  return combos.map((combo) => ({ combo, ...parse(combo, mac) }));
}

export function modifierMask(names) {
  if (!names) return 0;
  let mask = 0;
  for (const part of String(names).split("+")) {
    const bit = MODIFIERS[part.trim().toLowerCase()];
    if (!bit) refuse(`Unknown modifier "${part}". Use ctrl, shift, alt, cmd (meta) or win.`);
    mask |= bit;
  }
  return mask;
}

export function heldKeys(mask) {
  return [1, 2, 4, 8].filter((bit) => mask & bit).map((bit) => MODIFIER_KEYS[bit]);
}
