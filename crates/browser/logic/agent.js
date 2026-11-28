(() => {
  if (globalThis.__browserAgent) return;

  const refs = new Map();
  const ids = new WeakMap();
  let next = 1;

  const INTERACTIVE = new Set([
    "button", "link", "textbox", "searchbox", "combobox", "checkbox", "radio", "slider", "spinbutton", "switch",
    "tab", "menuitem", "menuitemcheckbox", "menuitemradio", "option", "listbox", "treeitem", "file",
  ]);
  const LANDMARKS = new Set(["navigation", "main", "banner", "contentinfo", "complementary", "form", "dialog", "search", "region"]);
  const SKIPPED_TAGS = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE", "META", "LINK", "HEAD", "BR", "WBR"]);
  const INPUT_ROLES = {
    button: "button", submit: "button", reset: "button", image: "button", checkbox: "checkbox", radio: "radio",
    range: "slider", number: "spinbutton", search: "searchbox", file: "file", hidden: null,
  };
  const TAG_ROLES = {
    A: (el) => (el.hasAttribute("href") ? "link" : "generic"),
    AREA: (el) => (el.hasAttribute("href") ? "link" : "generic"),
    BUTTON: () => "button", SUMMARY: () => "button", TEXTAREA: () => "textbox",
    SELECT: (el) => (el.multiple || el.size > 1 ? "listbox" : "combobox"), OPTION: () => "option",
    IMG: (el) => (el.getAttribute("alt") === "" ? "presentation" : "img"), SVG: () => "img", CANVAS: () => "canvas",
    H1: () => "heading", H2: () => "heading", H3: () => "heading", H4: () => "heading", H5: () => "heading", H6: () => "heading",
    NAV: () => "navigation", MAIN: () => "main", HEADER: () => "banner", FOOTER: () => "contentinfo", ASIDE: () => "complementary",
    FORM: () => "form", SECTION: (el) => (el.hasAttribute("aria-label") || el.hasAttribute("aria-labelledby") ? "region" : "generic"),
    ARTICLE: () => "article", DIALOG: () => "dialog", UL: () => "list", OL: () => "list", MENU: () => "list", LI: () => "listitem",
    TABLE: () => "table", TR: () => "row", TD: () => "cell", TH: () => "columnheader", P: () => "paragraph",
    LABEL: () => "label", FIELDSET: () => "group", DETAILS: () => "group", PROGRESS: () => "progressbar",
    METER: () => "meter", IFRAME: () => "iframe", VIDEO: () => "video", AUDIO: () => "audio", HR: () => "separator",
    BLOCKQUOTE: () => "blockquote", CODE: () => "code", PRE: () => "code", FIGURE: () => "figure", STRONG: () => "strong", EM: () => "emphasis",
  };

  function refOf(el) {
    const known = ids.get(el);
    if (known && refs.get(known)?.deref() === el) return known;
    const ref = `ref_${next++}`;
    ids.set(el, ref);
    refs.set(ref, new WeakRef(el));
    return ref;
  }

  function byRef(ref) {
    const el = refs.get(String(ref))?.deref();
    if (!el || !el.isConnected) {
      throw new Error(`Element ${ref} was not found. It may have been removed or the page changed; call read_page or find again for fresh references.`);
    }
    return el;
  }

  const PRIVATE_AUTOCOMPLETE = /(current-password|new-password|one-time-code|cc-number|cc-csc|cc-exp)/i;

  function sensitive(el) {
    return el.type === "password" || PRIVATE_AUTOCOMPLETE.test(el.getAttribute?.("autocomplete") || "") || Boolean(el.closest?.('[data-agent-private="true"]'));
  }

  function squash(text) {
    return String(text || "").replace(/\s+/g, " ").trim();
  }

  function clip(text, most) {
    return text.length > most ? `${text.slice(0, most)}…` : text;
  }

  function roleOf(el) {
    const explicit = el.getAttribute("role");
    if (explicit) return explicit.split(" ")[0];
    if (el.isContentEditable && (el.parentElement === null || !el.parentElement.isContentEditable)) return "textbox";
    if (el.tagName === "INPUT") {
      const type = (el.getAttribute("type") || "text").toLowerCase();
      if (type in INPUT_ROLES) return INPUT_ROLES[type];
      return el.hasAttribute("list") ? "combobox" : "textbox";
    }
    const byTag = TAG_ROLES[el.tagName.toUpperCase()];
    if (byTag) return byTag(el);
    if (el.hasAttribute("onclick") || (el.tabIndex >= 0 && el.hasAttribute("tabindex"))) return "button";
    return "generic";
  }

  function textOf(ids) {
    return squash(ids.split(/\s+/).map((id) => document.getElementById(id)?.textContent || "").join(" "));
  }

  function nameOf(el, role) {
    const labelledBy = el.getAttribute("aria-labelledby");
    if (labelledBy) {
      const named = textOf(labelledBy);
      if (named) return named;
    }
    const aria = squash(el.getAttribute("aria-label"));
    if (aria) return aria;
    if (["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName)) {
      const labels = el.labels ? Array.from(el.labels).map((label) => squash(label.innerText || label.textContent)).filter(Boolean) : [];
      if (labels.length) return labels.join(" ");
      if (el.type === "submit" || el.type === "button" || el.type === "reset") return squash(el.value) || squash(el.title);
      if (el.type === "image") return squash(el.alt) || squash(el.title);
      return squash(el.getAttribute("placeholder")) || squash(el.title) || squash(el.getAttribute("name"));
    }
    if (el.tagName === "IMG" || el.tagName === "AREA") return squash(el.getAttribute("alt")) || squash(el.title);
    if (el.tagName.toUpperCase() === "SVG") return squash(el.querySelector("title")?.textContent) || squash(el.getAttribute("title"));
    if (el.tagName === "FIELDSET") return squash(el.querySelector("legend")?.textContent);
    if (el.tagName === "TABLE") return squash(el.querySelector("caption")?.textContent);
    if (el.tagName === "IFRAME") return squash(el.title) || squash(el.getAttribute("name"));
    if (["button", "link", "heading", "tab", "menuitem", "option", "cell", "columnheader", "listitem", "label", "treeitem", "switch", "checkbox", "radio", "paragraph", "code", "blockquote", "strong", "emphasis"].includes(role)) {
      return clip(squash(el.innerText || el.textContent), role === "paragraph" ? 300 : 150) || squash(el.title);
    }
    return squash(el.title);
  }

  function hidden(el) {
    if (el.hidden || el.getAttribute("aria-hidden") === "true") return true;
    const style = getComputedStyle(el);
    return style.display === "none" || style.visibility === "hidden" || style.visibility === "collapse";
  }

  function inViewport(el) {
    const rect = el.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0 && rect.bottom > 0 && rect.right > 0 && rect.top < innerHeight && rect.left < innerWidth;
  }

  function attributes(el, role) {
    const found = [];
    const add = (key, value) => {
      if (value !== null && value !== undefined && value !== "") found.push(`${key}="${clip(squash(value), 120)}"`);
    };
    if (role === "link" && el.href) add("href", el.href);
    if (el.tagName === "INPUT") add("type", el.type);
    if (["textbox", "searchbox", "combobox", "spinbutton"].includes(role)) {
      add("placeholder", el.getAttribute("placeholder"));
      if (sensitive(el)) {
        if (el.value) add("value", "••••••");
      } else if (el.isContentEditable) {
        add("value", clip(squash(el.innerText), 80));
      } else if ("value" in el) {
        add("value", el.value);
      }
    }
    if (role === "slider") add("value", el.value);
    if (el.tagName === "SELECT") add("value", el.selectedOptions?.[0]?.text);
    if (role === "heading") {
      const level = el.getAttribute("aria-level") || el.tagName.match(/^H(\d)$/)?.[1];
      if (level) found.push(`level=${level}`);
    }
    if (role === "img" && el.tagName === "IMG" && !el.alt) add("src", el.currentSrc || el.src);
    const states = [];
    if (el.disabled || el.getAttribute("aria-disabled") === "true") states.push("disabled");
    if (el.checked === true || el.getAttribute("aria-checked") === "true") states.push("checked");
    if (el.selected === true || el.getAttribute("aria-selected") === "true") states.push("selected");
    if (el.getAttribute("aria-pressed") === "true") states.push("pressed");
    const expanded = el.getAttribute("aria-expanded") ?? (el.tagName === "DETAILS" ? String(el.open) : null);
    if (expanded !== null) states.push(`expanded=${expanded}`);
    if (el.required) states.push("required");
    if (el.readOnly) states.push("readonly");
    if (role !== "file" && el.type === "file") states.push("file");
    if (states.length) found.push(`(${states.join(", ")})`);
    return found;
  }

  function interactive(el, role) {
    return INTERACTIVE.has(role) || (el.tabIndex >= 0 && el.hasAttribute("tabindex") && role !== "generic") || el.isContentEditable;
  }

  function children(el) {
    const kids = [];
    if (el.shadowRoot) kids.push(...el.shadowRoot.children);
    if (el.tagName === "SLOT") kids.push(...el.assignedElements());
    kids.push(...el.children);
    return kids;
  }

  function ownText(el) {
    let text = "";
    for (const node of el.childNodes) if (node.nodeType === Node.TEXT_NODE) text += node.textContent;
    return squash(text);
  }

  const NAMED_FROM_CONTENT = new Set(["button", "link", "heading", "tab", "menuitem", "option", "cell", "columnheader", "listitem", "label", "treeitem", "checkbox", "radio", "switch", "paragraph", "code", "blockquote", "strong", "emphasis"]);

  function readTree({ filter, depth, refId }) {
    const lines = [];
    const onlyInteractive = filter === "interactive";
    const root = refId ? byRef(refId) : document.body || document.documentElement;
    const walk = (el, level, namedAbove) => {
      if (level > depth || SKIPPED_TAGS.has(el.tagName) || hidden(el)) return;
      const role = roleOf(el);
      if (role === null || role === "none" || role === "presentation") {
        for (const kid of children(el)) walk(kid, level, namedAbove);
        return;
      }
      const name = nameOf(el, role);
      const isInteractive = interactive(el, role);
      const meaningful = onlyInteractive ? isInteractive : role !== "generic" || name;
      let childLevel = level;
      if (meaningful && !(namedAbove && !isInteractive && role !== "img" && !onlyInteractive && NAMED_FROM_CONTENT.has(role) && !name)) {
        const pad = onlyInteractive ? "" : " ".repeat(level);
        const label = name ? ` "${clip(name, 150)}"` : "";
        const extra = attributes(el, role);
        lines.push(`${pad}${role}${label} [${refOf(el)}]${extra.length ? ` ${extra.join(" ")}` : ""}`);
        childLevel = level + 1;
      } else if (!onlyInteractive && role === "generic") {
        const text = ownText(el);
        if (text && !namedAbove) lines.push(`${" ".repeat(level)}text "${clip(text, 300)}"`);
      }
      const namesKids = NAMED_FROM_CONTENT.has(role) && Boolean(name);
      if (["INPUT", "TEXTAREA", "SELECT", "IFRAME", "SVG", "svg", "IMG"].includes(el.tagName)) return;
      for (const kid of children(el)) walk(kid, childLevel, namedAbove || namesKids);
    };
    if (root) walk(root, 0, false);
    return { lines };
  }

  function contextOf(el) {
    const trail = [];
    let node = el.parentElement;
    while (node && trail.length < 2) {
      const role = roleOf(node);
      if (LANDMARKS.has(role) || role === "listitem" || role === "row" || role === "article") {
        const heading = node.querySelector("h1,h2,h3,h4,h5,h6,[role=heading]");
        const label = squash(node.getAttribute("aria-label")) || (heading && heading !== el ? clip(squash(heading.innerText), 60) : "");
        trail.push(label ? `${role} "${label}"` : role);
      }
      node = node.parentElement;
    }
    return trail.join(" › ");
  }

  function candidates() {
    const found = [];
    const walk = (el) => {
      if (found.length >= 4000 || SKIPPED_TAGS.has(el.tagName) || hidden(el)) return;
      const role = roleOf(el);
      const isInteractive = interactive(el, role);
      const name = role ? nameOf(el, role) : "";
      const text = ownText(el);
      if (isInteractive || ["heading", "img", "label", "cell", "columnheader", "dialog", "navigation", "form", "search", "listitem", "article", "region"].includes(role) || (text && text.length <= 200)) {
        const attrs = {};
        for (const key of ["type", "placeholder", "id", "name", "title", "aria-label", "alt", "data-testid", "autocomplete"]) {
          const value = el.getAttribute(key);
          if (value) attrs[key] = clip(value, 120);
        }
        if (role === "link" && el.href) attrs.href = clip(el.href, 160);
        if ("value" in el && !sensitive(el) && typeof el.value === "string" && el.value) attrs.value = clip(el.value, 80);
        found.push({
          ref: refOf(el),
          role: role || "generic",
          name: clip(name, 150),
          text: clip(squash(el.innerText || el.textContent), 200),
          attrs,
          visible: inViewport(el),
          interactive: isInteractive,
          context: contextOf(el),
        });
      }
      if (["INPUT", "TEXTAREA", "SELECT", "SVG", "svg"].includes(el.tagName)) return;
      for (const kid of children(el)) walk(kid);
    };
    walk(document.body || document.documentElement);
    return { candidates: found, url: location.href };
  }

  function density(el) {
    const text = squash(el.innerText);
    if (!text) return 0;
    const links = Array.from(el.querySelectorAll("a")).reduce((sum, link) => sum + squash(link.innerText).length, 0);
    const paragraphs = el.querySelectorAll("p").length;
    return (text.length - links * 0.8) + paragraphs * 60;
  }

  function pageText() {
    const preferred = ["article", "main", "[role=main]", "#content", "#main", ".post", ".article", ".content"];
    let source = null;
    for (const selector of preferred) {
      const found = Array.from(document.querySelectorAll(selector)).filter((el) => !hidden(el));
      const best = found.sort((a, b) => density(b) - density(a))[0];
      if (best && squash(best.innerText).length > 200) {
        source = best;
        break;
      }
    }
    if (!source) {
      const blocks = Array.from(document.querySelectorAll("div, section")).filter((el) => !hidden(el) && el.querySelectorAll("p").length >= 3);
      source = blocks.sort((a, b) => density(b) - density(a))[0] || null;
      const bodyLength = squash(document.body?.innerText).length;
      if (source && squash(source.innerText).length < bodyLength * 0.25) source = null;
    }
    source = source || document.body || document.documentElement;
    const text = String(source.innerText || source.textContent || "")
      .replace(/[ \t ]+/g, " ")
      .replace(/ *\n */g, "\n")
      .replace(/\n{3,}/g, "\n\n")
      .trim();
    const tag = source.tagName.toLowerCase();
    const described = source.id ? `${tag} id="${source.id}"` : source.getAttribute("role") ? `${tag} role="${source.getAttribute("role")}"` : tag;
    return { title: document.title, url: location.href, source: described, text };
  }

  function fire(el, type) {
    el.dispatchEvent(new Event(type, { bubbles: true }));
  }

  function nativeSet(el, value) {
    const proto = el.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : el.tagName === "SELECT" ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
    if (setter) setter.call(el, value);
    else el.value = value;
  }

  function truthy(value) {
    if (typeof value === "boolean") return value;
    return ["true", "1", "yes", "on", "checked"].includes(String(value).trim().toLowerCase());
  }

  function formInput(ref, value) {
    const el = byRef(ref);
    const role = roleOf(el);
    el.scrollIntoView({ block: "center", inline: "nearest", behavior: "instant" });
    if (el.disabled) throw new Error(`${ref} is disabled, so its value cannot be set.`);
    if (el.tagName === "SELECT") {
      const wanted = squash(value).toLowerCase();
      const options = Array.from(el.options);
      const option = options.find((o) => o.value === String(value)) || options.find((o) => squash(o.text).toLowerCase() === wanted) || options.find((o) => squash(o.text).toLowerCase().includes(wanted));
      if (!option) {
        const listed = options.slice(0, 30).map((o) => `"${squash(o.text)}" (value "${o.value}")`).join(", ");
        throw new Error(`No option matches "${value}" in ${ref}. Options: ${listed}`);
      }
      el.focus();
      nativeSet(el, option.value);
      option.selected = true;
      fire(el, "input");
      fire(el, "change");
      return `Selected option "${squash(option.text)}" in ${ref}`;
    }
    if (el.tagName === "INPUT" && (el.type === "checkbox" || el.type === "radio")) {
      const want = truthy(value);
      if (el.checked !== want) el.click();
      if (el.checked !== want) {
        el.checked = want;
        fire(el, "input");
        fire(el, "change");
      }
      return `${want ? "Checked" : "Unchecked"} ${ref}`;
    }
    if (["checkbox", "switch", "radio"].includes(role)) {
      const want = truthy(value);
      if ((el.getAttribute("aria-checked") === "true") !== want) el.click();
      return `${want ? "Checked" : "Unchecked"} ${ref}`;
    }
    if (el.tagName === "INPUT" && el.type === "file") {
      throw new Error(`${ref} is a file input; use the file_upload tool (or upload_image for a screenshot) to set its files.`);
    }
    if (el.tagName === "INPUT" || el.tagName === "TEXTAREA") {
      el.focus();
      nativeSet(el, String(value));
      fire(el, "input");
      fire(el, "change");
      const shown = sensitive(el) ? "••••••" : String(value);
      return `Set value of ${ref} to "${clip(shown, 200)}"`;
    }
    if (el.isContentEditable) {
      el.focus();
      const selection = getSelection();
      const range = document.createRange();
      range.selectNodeContents(el);
      selection.removeAllRanges();
      selection.addRange(range);
      if (!document.execCommand("insertText", false, String(value))) el.textContent = String(value);
      fire(el, "input");
      return `Set content of ${ref} to "${clip(String(value), 200)}"`;
    }
    throw new Error(`${ref} is a ${role}, not a form field. Use the computer tool's left_click (with ref) to interact with it.`);
  }

  function refPoint(ref) {
    const el = byRef(ref);
    if (!inViewport(el)) el.scrollIntoView({ block: "center", inline: "center", behavior: "instant" });
    const rect = el.getBoundingClientRect();
    if (rect.width === 0 && rect.height === 0) {
      throw new Error(`${ref} has no size on the page (it may be hidden or collapsed). Try a different element, or a screenshot to find what to click.`);
    }
    const x = Math.min(Math.max(rect.left + rect.width / 2, 0), innerWidth - 1);
    const y = Math.min(Math.max(rect.top + rect.height / 2, 0), innerHeight - 1);
    return { x, y, width: rect.width, height: rect.height };
  }

  function scrollTo(ref) {
    const el = byRef(ref);
    el.scrollIntoView({ block: "center", inline: "nearest", behavior: "instant" });
    const rect = el.getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  }

  function toFiles(files) {
    return files.map((file) => {
      const binary = atob(file.data);
      const bytes = new Uint8Array(binary.length);
      for (let at = 0; at < binary.length; at += 1) bytes[at] = binary.charCodeAt(at);
      return new File([bytes], file.name, { type: file.mimeType || "application/octet-stream", lastModified: Date.now() });
    });
  }

  function fileInputFor(el) {
    if (el.tagName === "INPUT" && el.type === "file") return el;
    if (el.tagName === "LABEL" && el.control?.type === "file") return el.control;
    return el.querySelector?.("input[type=file]") || null;
  }

  function putFiles(target, files) {
    const list = toFiles(files);
    const transfer = new DataTransfer();
    for (const file of list) transfer.items.add(file);
    const el = target.ref ? byRef(target.ref) : document.elementFromPoint(target.x, target.y);
    if (!el) throw new Error(`There is no element at (${Math.round(target.x)}, ${Math.round(target.y)}) to drop the file on.`);
    const input = target.ref ? fileInputFor(el) : null;
    if (input) {
      if (!input.multiple && list.length > 1) throw new Error(`${target.ref} accepts a single file; upload one at a time.`);
      input.files = transfer.files;
      fire(input, "input");
      fire(input, "change");
      return { how: "input", names: list.map((file) => file.name) };
    }
    const rect = el.getBoundingClientRect();
    const clientX = target.ref ? rect.left + rect.width / 2 : target.x;
    const clientY = target.ref ? rect.top + rect.height / 2 : target.y;
    for (const type of ["dragenter", "dragover", "drop"]) {
      el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, composed: true, dataTransfer: transfer, clientX, clientY }));
    }
    return { how: "drop", names: list.map((file) => file.name), on: `${roleOf(el)}${nameOf(el, roleOf(el)) ? ` "${clip(nameOf(el, roleOf(el)), 60)}"` : ""}` };
  }

  const API = { readTree, candidates, pageText, formInput, refPoint, scrollTo, putFiles };

  globalThis.__browserAgent = {
    call(name, args) {
      try {
        return { value: API[name](...args) };
      } catch (error) {
        return { error: String(error?.message || error) };
      }
    },
  };
})();
