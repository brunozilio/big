const SHOWN = 60_000;
const ACCEPT = "text/markdown, text/plain;q=0.9, text/html;q=0.8, application/json;q=0.8, */*;q=0.5";
const THIN_TEXT = 400;
const THIN_HTML = 10_000;
const DEEPEST = 256;
const SUMMARY = 300;
const LONGEST = 200;
const SHOWN_OF_A_RUN = 8;
const SHORT = 30;
const REPEATED = 5;
const MIN_PAGE = 3000;
const MIN_SHARE = 0.25;
const DESCEND_SHARE = 0.6;
const KEEP_SHARE = 0.5;
const MAX_DEPTH = 12;
const JSON_DEPTH = 128;

export const spec = {
  name: "WebFetch",
  input: {
    "additionalProperties": false,
    "properties": {
      "prompt": {
        "description": "What to find on it.",
        "type": "string"
      },
      "url": {
        "description": "The page's full URL.",
        "type": "string"
      }
    },
    "required": [
      "url",
      "prompt"
    ],
    "type": "object"
  },
  description: `Fetch a web page (http upgraded to https) and read it as text, with what you are looking for in \`prompt\`. The main content comes first and links are numbered with their addresses listed at the end; JSON and feeds come back compacted. Long pages are cut; fetch a narrower page then. A page built only by JavaScript returns an error.`,
  profile: { effects: ["read"], risk: "low", reversible: true, untrusted: true, sensitive: false },
};

type Input = { url: string; prompt: string };

type Reading = {
  mainContent: boolean;
  cleanup: boolean;
  links: boolean;
  formats: boolean;
  emptyPageError: boolean;
  configured?: Reading;
};

type Element = { name: string; attrs: [string, string][]; children: Node[] };
type Node = Element | string;

type Url = {
  scheme: string;
  web: boolean;
  user: string;
  host: string;
  port: string;
  path: string;
  query: string | null;
  fragment: string | null;
};

export function effects(input: Input | null) {
  return [{ fetch: site(input?.url ?? "") }];
}

export function summary(input: Input | null) {
  return input?.url ?? "";
}

export function execute(input: Input, ctx) {
  if (typeof input?.url !== "string") {
    return { content: "Missing required parameter `url`.", isError: true };
  }
  const reading: Reading = ctx.http.reading();
  let fetched: { status: number; url: string; contentType: string | null; body: string };
  try {
    fetched = ctx.http.fetch(reading.formats ? { url: input.url, accept: ACCEPT } : { url: input.url });
  } catch (error) {
    return { content: String(error.message ?? error), isError: true };
  }
  if (fetched.status < 200 || fetched.status >= 300) {
    return { content: `${fetched.url} answered ${fetched.status}`, isError: true };
  }
  const read = readPage(fetched.body, fetched.contentType ?? null, fetched.url, reading);
  if (read.error !== undefined) {
    return { content: read.error, isError: true };
  }
  return clipped(read.text, SHOWN);
}

function site(url: string): string | null {
  const scheme = url.indexOf("://");
  const rest = scheme < 0 ? url : url.slice(scheme + 3);
  let host = rest.slice(0, scan(rest, 0, authorityEnd));
  const at = host.lastIndexOf("@");
  if (at >= 0) {
    host = host.slice(at + 1);
  }
  if (host.startsWith("[")) {
    const close = host.indexOf("]");
    host = close < 0 ? host.slice(1) : host.slice(1, close);
  } else {
    host = host.split(":")[0];
  }
  host = asciiLower(trimEndAll(host, "."));
  return host === "" ? null : host;
}

function isSpace(code: number) {
  return (
    (code >= 9 && code <= 13) ||
    code === 32 ||
    code === 0x85 ||
    code === 0xa0 ||
    code === 0x1680 ||
    (code >= 0x2000 && code <= 0x200a) ||
    code === 0x2028 ||
    code === 0x2029 ||
    code === 0x202f ||
    code === 0x205f ||
    code === 0x3000
  );
}

function scan(text: string, at: number, stop: (code: number) => boolean) {
  let index = at;
  while (index < text.length && !stop(text.charCodeAt(index))) {
    index++;
  }
  return index;
}

const solid = (code: number) => !isSpace(code);
const attrStart = (code: number) => !isSpace(code) && code !== 47;
const attrNameEnd = (code: number) => isSpace(code) || code === 61 || code === 47;
const tagNameEnd = (code: number) => isSpace(code) || code === 47 || code === 62;
const slashless = (code: number) => code !== 47 && code !== 92;
const authorityEnd = (code: number) => code === 47 || code === 92 || code === 63 || code === 35;
const highSurrogate = (code: number) => code >= 0xd800 && code <= 0xdbff;

function replaceAll(text: string, from: string, to: string) {
  return text.indexOf(from) < 0 ? text : text.split(from).join(to);
}

function trimStart(text: string) {
  let at = 0;
  while (at < text.length && isSpace(text.charCodeAt(at))) {
    at++;
  }
  return at === 0 ? text : text.slice(at);
}

function trimEnd(text: string) {
  let end = text.length;
  while (end > 0 && isSpace(text.charCodeAt(end - 1))) {
    end--;
  }
  return end === text.length ? text : text.slice(0, end);
}

function trim(text: string) {
  return trimEnd(trimStart(text));
}

function words(text: string) {
  const found: string[] = [];
  let at = scan(text, 0, solid);
  while (at < text.length) {
    const end = scan(text, at, isSpace);
    found.push(text.slice(at, end));
    at = scan(text, end, solid);
  }
  return found;
}
function squash(text: string) {
  return words(text).join(" ");
}
function asciiLower(text: string) {
  const first = scan(text, 0, (code) => code >= 65 && code <= 90);
  if (first === text.length) {
    return text;
  }
  const out: string[] = [text.slice(0, first)];
  for (let at = first; at < text.length; at++) {
    const code = text.charCodeAt(at);
    out.push(code >= 65 && code <= 90 ? String.fromCharCode(code + 32) : text[at]);
  }
  return out.join("");
}
function trimEndAll(text: string, suffix: string) {
  let end = text.length;
  while (suffix !== "" && end >= suffix.length && text.startsWith(suffix, end - suffix.length)) {
    end -= suffix.length;
  }
  return end === text.length ? text : text.slice(0, end);
}
function trimAll(text: string, mark: string) {
  let start = 0;
  let end = text.length;
  while (start < end && text[start] === mark) {
    start++;
  }
  while (end > start && text[end - 1] === mark) {
    end--;
  }
  return text.slice(start, end);
}

function utf8Length(text: string) {
  let total = 0;
  for (let at = 0; at < text.length; at++) {
    const code = text.charCodeAt(at);
    if (code < 0x80) {
      total += 1;
    } else if (code < 0x800) {
      total += 2;
    } else if (highSurrogate(code) && at + 1 < text.length) {
      total += 4;
      at++;
    } else {
      total += 3;
    }
  }
  return total;
}
function charCount(text: string) {
  let count = 0;
  for (let at = 0; at < text.length; at++) {
    if (highSurrogate(text.charCodeAt(at)) && at + 1 < text.length) {
      at++;
    }
    count++;
  }
  return count;
}
function charIndex(text: string, nth: number) {
  let at = 0;
  for (let count = 0; count < nth; count++) {
    if (at >= text.length) {
      return -1;
    }
    at += highSurrogate(text.charCodeAt(at)) && at + 1 < text.length ? 2 : 1;
  }
  return at < text.length ? at : -1;
}
function lines(text: string) {
  if (text === "") {
    return [];
  }
  const found = text.split("\n");
  if (found[found.length - 1] === "") {
    found.pop();
  }
  return found.map((line) => (line.endsWith("\r") ? line.slice(0, -1) : line));
}

function clip(text: string, limit: number) {
  const at = charIndex(text, limit);
  if (at < 0) {
    return text;
  }
  return `${text.slice(0, at)}\n… [${charCount(text.slice(at))} more characters cut]`;
}

const NAMED = new Map([
  ["amp", "&"],
  ["lt", "<"],
  ["gt", ">"],
  ["quot", '"'],
  ["apos", "'"],
  ["nbsp", " "],
  ["mdash", "—"],
  ["ndash", "–"],
  ["hellip", "…"],
  ["copy", "©"],
]);

function decode(text: string) {
  if (text.indexOf("&") < 0) {
    return text;
  }
  const out: string[] = [];
  let from = 0;
  let at = text.indexOf("&", from);
  while (at >= 0) {
    out.push(text.slice(from, at));
    const end = semicolon(text, at);
    const found = end < 0 ? null : entity(text.slice(at + 1, end));
    if (found === null) {
      out.push("&");
      from = at + 1;
    } else {
      out.push(found);
      from = end + 1;
    }
    at = text.indexOf("&", from);
  }
  out.push(text.slice(from));
  return out.join("");
}

function semicolon(text: string, at: number) {
  let index = at;
  for (let count = 0; count < 12 && index < text.length; count++) {
    const code = text.charCodeAt(index);
    if (code === 59) {
      return index;
    }
    index += code >= 0xd800 && code <= 0xdbff ? 2 : 1;
  }
  return -1;
}

function entity(name: string) {
  const named = NAMED.get(name);
  if (named !== undefined) {
    return named;
  }
  let code: number | null = null;
  if (name.startsWith("#x") || name.startsWith("#X")) {
    code = numeric(name.slice(2), 16);
  }
  if (code === null && name.startsWith("#")) {
    code = numeric(name.slice(1), 10);
  }
  if (code === null || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) {
    return null;
  }
  return String.fromCodePoint(code);
}

function numeric(digits: string, radix: number) {
  const pattern = radix === 16 ? /^\+?[0-9a-fA-F]+$/ : /^\+?[0-9]+$/;
  if (!pattern.test(digits)) {
    return null;
  }
  const value = parseInt(digits.replace("+", ""), radix);
  return value <= 0xffffffff ? value : null;
}

function parseAttrs(source: string) {
  const attrs: [string, string][] = [];
  const rest = trimEndAll(source, "/");
  let at = 0;
  for (;;) {
    at = scan(rest, at, attrStart);
    if (at >= rest.length) {
      return attrs;
    }
    const nameEnd = scan(rest, at, attrNameEnd);
    const name = asciiLower(rest.slice(at, nameEnd));
    at = scan(rest, nameEnd, solid);
    if (rest[at] !== "=") {
      attrs.push([name, ""]);
      continue;
    }
    at = scan(rest, at + 1, solid);
    const quote = rest[at];
    if (quote === '"' || quote === "'") {
      const end = rest.indexOf(quote, at + 1);
      attrs.push([name, decode(rest.slice(at + 1, end < 0 ? rest.length : end))]);
      at = end < 0 ? rest.length : end + 1;
    } else {
      const end = scan(rest, at, isSpace);
      attrs.push([name, decode(rest.slice(at, end))]);
      at = end;
    }
  }
}
const VOID = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"]);
const RAW = new Set(["script", "style", "textarea", "title", "noscript"]);
const CLOSED_BY_SELF = new Set(["p", "li", "option", "tr", "td", "th", "dt", "dd"]);

function attr(element: Element, name: string) {
  for (const [key, value] of element.attrs) {
    if (key === name) {
      return value;
    }
  }
  return null;
}

function elements(element: Element): Element[] {
  return element.children.filter((child): child is Element => typeof child !== "string");
}

function find(element: Element, matches: (element: Element) => boolean) {
  const found: Element[] = [];
  const pending: Node[] = element.children.slice().reverse();
  while (pending.length > 0) {
    const node = pending.pop() as Node;
    if (typeof node === "string") {
      continue;
    }
    if (matches(node)) {
      found.push(node);
    }
    for (let at = node.children.length - 1; at >= 0; at--) {
      pending.push(node.children[at]);
    }
  }
  return found;
}
function first(element: Element, name: string): Element | null {
  const pending: Node[] = element.children.slice().reverse();
  while (pending.length > 0) {
    const node = pending.pop() as Node;
    if (typeof node === "string") {
      continue;
    }
    if (node.name === name) {
      return node;
    }
    for (let at = node.children.length - 1; at >= 0; at--) {
      pending.push(node.children[at]);
    }
  }
  return null;
}
function text(element: Element) {
  const parts: string[] = [];
  gather(element, parts);
  return squash(parts.join(" "));
}

function gather(element: Element, parts: string[]) {
  const pending: Node[] = element.children.slice().reverse();
  while (pending.length > 0) {
    const node = pending.pop() as Node;
    if (typeof node === "string") {
      parts.push(node);
    } else if (node.name !== "script" && node.name !== "style") {
      for (let at = node.children.length - 1; at >= 0; at--) {
        pending.push(node.children[at]);
      }
    }
  }
}
const NO_ATTRS: [string, string][] = Object.freeze([]) as unknown as [string, string][];
const NO_CHILDREN: Node[] = Object.freeze([]) as unknown as Node[];

type Open = { stack: Element[]; named: Map<string, number> };

function addNode(open: Open, node: Node) {
  open.stack[open.stack.length - 1].children.push(node);
}

function addText(open: Open, raw: string) {
  if (raw !== "") {
    addNode(open, decode(raw));
  }
}

const SETTLED = new Map<string, Element>();

function settled(element: Element) {
  if (element.attrs !== NO_ATTRS || element.children !== NO_CHILDREN) {
    return element;
  }
  const known = SETTLED.get(element.name);
  if (known !== undefined) {
    return known;
  }
  SETTLED.set(element.name, element);
  return element;
}

function push(open: Open, element: Element) {
  element.children = [];
  open.named.set(element.name, (open.named.get(element.name) ?? 0) + 1);
  open.stack.push(element);
}

function holds(open: Open, name: string) {
  return (open.named.get(name) ?? 0) > 0;
}

function fold(open: Open) {
  if (open.stack.length < 2) {
    return;
  }
  const done = open.stack.pop() as Element;
  const count = open.named.get(done.name);
  if (count !== undefined) {
    open.named.set(done.name, Math.max(0, count - 1));
  }
  addNode(open, done);
}

function foldTo(open: Open, at: number) {
  while (open.stack.length > Math.max(at, 1)) {
    fold(open);
  }
}

function lastNamed(open: Open, names: (name: string) => boolean) {
  for (let at = open.stack.length - 1; at >= 0; at--) {
    if (names(open.stack[at].name)) {
      return at;
    }
  }
  return -1;
}

function close(open: Open, name: string) {
  if (!holds(open, name)) {
    return;
  }
  const at = lastNamed(open, (known) => known === name);
  if (at > 0) {
    foldTo(open, at);
  }
}

function scopeOf(name: string) {
  switch (name) {
    case "li":
      return ["ul", "ol"];
    case "td":
    case "th":
    case "tr":
      return ["table", "tbody", "thead"];
    case "option":
      return ["select", "datalist"];
    default:
      return ["div", "section", "article", "main", "body", "td", "li", "blockquote"];
  }
}

function closeSibling(open: Open, name: string) {
  if (!holds(open, name)) {
    return;
  }
  const scope = scopeOf(name);
  const at = lastNamed(open, (known) => known === name);
  const fence = lastNamed(open, (known) => scope.includes(known));
  if (at >= 0 && (fence < 0 || at > fence)) {
    foldTo(open, at);
  }
}

function parse(source: string, xml: boolean): Element {
  const open: Open = { stack: [{ name: "#document", attrs: [], children: [] }], named: new Map() };
  let pos = 0;
  while (pos < source.length) {
    const at = source.indexOf("<", pos);
    if (at < 0) {
      addText(open, source.slice(pos));
      break;
    }
    addText(open, source.slice(pos, at));
    pos = step(open, source, at, xml);
  }
  foldTo(open, 1);
  return open.stack.pop() ?? { name: "", attrs: [], children: [] };
}

function step(open: Open, source: string, pos: number, xml: boolean) {
  const n = source.length;
  if (source.startsWith("<![CDATA[", pos)) {
    const end = source.indexOf("]]>", pos + 9);
    addNode(open, source.slice(pos + 9, end < 0 ? n : end));
    return end < 0 ? n : end + 3;
  }
  if (source.startsWith("<!--", pos)) {
    const end = source.indexOf("-->", pos + 4);
    return end < 0 ? n : end + 3;
  }
  if (source.startsWith("<!", pos) || source.startsWith("<?", pos)) {
    const end = source.indexOf(">", pos);
    return end < 0 ? n : end + 1;
  }
  const end = tagEnd(source, pos);
  if (end < 0) {
    addText(open, source.slice(pos));
    return n;
  }
  return opening(open, source, source.slice(pos + 1, end), end + 1, xml);
}

function opening(open: Open, source: string, tag: string, pos: number, xml: boolean) {
  if (tag.startsWith("/")) {
    close(open, nameOf(tag.slice(1), xml));
    return pos;
  }
  const element = opened(tag, xml);
  if (element === null) {
    addText(open, "<");
    return pos;
  }
  const leaf = tag.endsWith("/") || (!xml && VOID.has(element.name));
  if (!xml && RAW.has(element.name)) {
    const end = closingTag(source, pos, element.name);
    if (element.name !== "script" && element.name !== "style") {
      const inside = decode(source.slice(pos, end));
      element.children = inside === "" ? NO_CHILDREN : [inside];
    }
    addNode(open, settled(element));
    const closed = source.indexOf(">", end);
    return closed < 0 ? source.length : closed + 1;
  }
  if (leaf) {
    addNode(open, settled(element));
    return pos;
  }
  if (!xml && CLOSED_BY_SELF.has(element.name)) {
    closeSibling(open, element.name);
  }
  if (open.stack.length <= DEEPEST) {
    push(open, element);
  }
  return pos;
}

function closingTag(source: string, from: number, name: string) {
  let at = source.indexOf("</", from);
  while (at >= 0) {
    if (asciiLower(source.substr(at + 2, name.length)) === name) {
      return at;
    }
    at = source.indexOf("</", at + 2);
  }
  return source.length;
}

function tagEnd(source: string, pos: number) {
  let quote = 0;
  for (let at = pos + 1; at < source.length; at++) {
    const code = source.charCodeAt(at);
    if (quote === 0) {
      if (code === 34 || code === 39) {
        quote = code;
      } else if (code === 62) {
        return at;
      }
    } else if (code === quote) {
      quote = 0;
    }
  }
  return -1;
}
function nameEnd(tag: string) {
  return scan(tag, 0, tagNameEnd);
}
function nameOf(tag: string, xml: boolean, end = nameEnd(tag)) {
  let name = tag.slice(0, end);
  if (xml) {
    name = name.slice(name.lastIndexOf(":") + 1);
  }
  return asciiLower(name);
}
function opened(tag: string, xml: boolean): Element | null {
  const end = nameEnd(tag);
  const name = nameOf(tag, xml, end);
  const lead = name.charCodeAt(0) | 32;
  if (name === "" || lead < 97 || lead > 122) {
    return null;
  }
  return { name, attrs: end === tag.length ? NO_ATTRS : parseAttrs(tag.slice(end)), children: NO_CHILDREN };
}
const COLUMNS = new Set(["div", "section", "article", "main", "td", "center"]);
const FURNITURE = new Set(["nav", "header", "footer", "aside"]);
const NEVER_TEXT = new Set(["script", "style", "noscript", "svg", "head", "template"]);
const DROPPED = new Set([
  "script",
  "style",
  "noscript",
  "svg",
  "head",
  "template",
  "iframe",
  "button",
  "select",
  "input",
  "textarea",
  "canvas",
  "object",
  "embed",
]);
const BLOCKS = new Set([
  "p",
  "div",
  "section",
  "article",
  "main",
  "table",
  "ul",
  "ol",
  "dl",
  "dt",
  "dd",
  "blockquote",
  "figure",
  "figcaption",
  "form",
  "details",
  "summary",
  "hr",
  "address",
  "fieldset",
]);

const HIDDEN = new WeakMap<Element, boolean>();

function hidden(element: Element) {
  const known = HIDDEN.get(element);
  if (known !== undefined) {
    return known;
  }
  const style = attr(element, "style");
  const found =
    attr(element, "hidden") !== null ||
    attr(element, "aria-hidden") === "true" ||
    (style !== null && asciiLower(words(style).join("")).includes("display:none"));
  HIDDEN.set(element, found);
  return found;
}

function mainOf(body: Element): Element | null {
  const total = prose(body);
  if (utf8Length(text(body)) < MIN_PAGE) {
    return null;
  }
  const landmarks: ((element: Element) => boolean)[] = [
    (element) => element.name === "main",
    (element) => attr(element, "role") === "main",
    (element) => element.name === "article",
  ];
  for (const landmark of landmarks) {
    const found = find(body, landmark);
    if (found.length === 1 && prose(found[0]) >= total * MIN_SHARE) {
      return densest(found[0], prose(found[0])) ?? found[0];
    }
  }
  return densest(body, total);
}

function densest(body: Element, total: number): Element | null {
  let node = body;
  for (let depth = 0; depth < MAX_DEPTH; depth++) {
    let best: Element | null = null;
    let most = -1;
    for (const child of elements(node)) {
      if (COLUMNS.has(child.name)) {
        const length = prose(child);
        if (length >= most) {
          most = length;
          best = child;
        }
      }
    }
    if (best === null || most < prose(node) * DESCEND_SHARE || most < total * KEEP_SHARE) {
      break;
    }
    node = best;
  }
  return node === body ? null : node;
}

const PROSE = new WeakMap<Element, number>();

function prose(element: Element): number {
  const known = PROSE.get(element);
  if (known !== undefined) {
    return known;
  }
  let length = 0;
  const pending: Node[] = element.children.slice();
  while (pending.length > 0) {
    const node = pending.pop() as Node;
    if (typeof node === "string") {
      const squashed = squash(node);
      length += squashed === "" ? 0 : utf8Length(squashed) + 1;
    } else if (node.name !== "a" && node.name !== "select" && node.name !== "button" && !DROPPED.has(node.name) && !hidden(node)) {
      for (const child of node.children) {
        pending.push(child);
      }
    }
  }
  PROSE.set(element, length);
  return length;
}
function repeatedLabels(body: Element) {
  const counts = new Map<string, number>();
  for (const link of find(body, (element) => element.name === "a")) {
    const label = text(link).toLowerCase();
    if (label !== "" && utf8Length(label) <= SHORT) {
      counts.set(label, (counts.get(label) ?? 0) + 1);
    }
  }
  return new Set([...counts].filter(([, count]) => count >= REPEATED).map(([label]) => label));
}

function linkRun(list: Element): Element[] | null {
  const links: Element[] = [];
  for (const child of list.children) {
    if (typeof child === "string") {
      if (trim(child) === "") {
        continue;
      }
      return null;
    }
    const link = loneLink(child);
    if (link === null) {
      return null;
    }
    links.push(link);
  }
  return links.length > SHOWN_OF_A_RUN ? links : null;
}

function loneLink(item: Element): Element | null {
  let link = item;
  if (item.name !== "a") {
    const found = find(item, (element) => element.name === "a");
    if (found.length !== 1 || text(found[0]) !== text(item)) {
      return null;
    }
    link = found[0];
  }
  return attr(link, "href") !== null && utf8Length(text(link)) <= SHORT ? link : null;
}

function utf8(text: string) {
  const bytes: number[] = [];
  for (const char of text) {
    let code = char.codePointAt(0) ?? 0xfffd;
    if (code >= 0xd800 && code <= 0xdfff) {
      code = 0xfffd;
    }
    if (code < 0x80) {
      bytes.push(code);
    } else if (code < 0x800) {
      bytes.push(0xc0 | (code >> 6), 0x80 | (code & 63));
    } else if (code < 0x10000) {
      bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 63), 0x80 | (code & 63));
    } else {
      bytes.push(0xf0 | (code >> 18), 0x80 | ((code >> 12) & 63), 0x80 | ((code >> 6) & 63), 0x80 | (code & 63));
    }
  }
  return bytes;
}

function leadOf(byte: number) {
  if (byte >= 0xc2 && byte <= 0xdf) {
    return { need: 1, code: byte & 0x1f, lower: 0x80, upper: 0xbf };
  }
  if (byte >= 0xe0 && byte <= 0xef) {
    return { need: 2, code: byte & 0x0f, lower: byte === 0xe0 ? 0xa0 : 0x80, upper: byte === 0xed ? 0x9f : 0xbf };
  }
  if (byte >= 0xf0 && byte <= 0xf4) {
    return { need: 3, code: byte & 0x07, lower: byte === 0xf0 ? 0x90 : 0x80, upper: byte === 0xf4 ? 0x8f : 0xbf };
  }
  return null;
}

function fromUtf8(bytes: number[]) {
  const out: string[] = [];
  let at = 0;
  while (at < bytes.length) {
    const byte = bytes[at];
    if (byte < 0x80) {
      out.push(String.fromCharCode(byte));
      at++;
      continue;
    }
    const lead = leadOf(byte);
    at++;
    if (lead === null) {
      out.push("\ufffd");
      continue;
    }
    let { code, lower, upper } = lead;
    let whole = true;
    for (let count = 0; count < lead.need; count++) {
      const next = bytes[at];
      if (next === undefined || next < lower || next > upper) {
        whole = false;
        break;
      }
      code = (code << 6) | (next & 0x3f);
      lower = 0x80;
      upper = 0xbf;
      at++;
    }
    out.push(whole ? String.fromCodePoint(code) : "\ufffd");
  }
  return out.join("");
}

function hex(byte: number) {
  return `%${byte < 16 ? "0" : ""}${byte.toString(16).toUpperCase()}`;
}

function percentEncode(text: string, kept: (code: number) => boolean) {
  if (scan(text, 0, (code) => code <= 0x20 || code >= 0x7f || !kept(code)) === text.length) {
    return text;
  }
  const out: string[] = [];
  for (const char of text) {
    const code = char.codePointAt(0) ?? 0;
    if (code > 0x20 && code < 0x7f && kept(code)) {
      out.push(char);
    } else {
      out.push(utf8(char).map(hex).join(""));
    }
  }
  return out.join("");
}

const PATH_ENCODED = ' "#<>?`{}';
const QUERY_ENCODED = " \"#<>'";
const FRAGMENT_ENCODED = ' "<>`';

function percentDecode(text: string) {
  const bytes = utf8(text);
  const out: number[] = [];
  for (let at = 0; at < bytes.length; at++) {
    const pair = String.fromCharCode(bytes[at + 1] ?? 0, bytes[at + 2] ?? 0);
    if (bytes[at] === 37 && /^[0-9a-fA-F]{2}$/.test(pair)) {
      out.push(parseInt(pair, 16));
      at += 2;
    } else {
      out.push(bytes[at]);
    }
  }
  return fromUtf8(out);
}

function formPairs(query: string) {
  return query
    .split("&")
    .filter((piece) => piece !== "")
    .map((piece) => {
      const equals = piece.indexOf("=");
      const [key, value] = equals < 0 ? [piece, ""] : [piece.slice(0, equals), piece.slice(equals + 1)];
      return [percentDecode(replaceAll(key, "+", " ")), percentDecode(replaceAll(value, "+", " "))];
    });
}

function formEncode(text: string) {
  return utf8(text)
    .map((byte) => {
      if (byte === 0x20) {
        return "+";
      }
      const char = String.fromCharCode(byte);
      return /^[A-Za-z0-9*\-._]$/.test(char) ? char : hex(byte);
    })
    .join("");
}

function cleanedUrl(input: string) {
  let start = 0;
  let end = input.length;
  while (start < end && input.charCodeAt(start) <= 0x20) {
    start++;
  }
  while (end > start && input.charCodeAt(end - 1) <= 0x20) {
    end--;
  }
  return replaceAll(replaceAll(replaceAll(input.slice(start, end), "\t", ""), "\n", ""), "\r", "");
}

function splitTail(text: string) {
  const hash = text.indexOf("#");
  const fragment = hash < 0 ? null : text.slice(hash + 1);
  const before = hash < 0 ? text : text.slice(0, hash);
  const mark = before.indexOf("?");
  return {
    path: mark < 0 ? before : before.slice(0, mark),
    query: mark < 0 ? null : before.slice(mark + 1),
    fragment,
  };
}

function encodedTail(url: Url, query: string | null, fragment: string | null): Url {
  return {
    ...url,
    query: query === null ? null : percentEncode(query, (code) => !QUERY_ENCODED.includes(String.fromCharCode(code))),
    fragment:
      fragment === null ? null : percentEncode(fragment, (code) => !FRAGMENT_ENCODED.includes(String.fromCharCode(code))),
  };
}

function dots(segment: string) {
  const lower = asciiLower(segment);
  if (lower === "." || lower === "%2e") {
    return 1;
  }
  if (lower === ".." || lower === ".%2e" || lower === "%2e." || lower === "%2e%2e") {
    return 2;
  }
  return 0;
}

function normalPath(path: string) {
  const segments = replaceAll(path, "\\", "/").split("/").slice(1);
  const kept: string[] = [];
  segments.forEach((segment, at) => {
    const last = at === segments.length - 1;
    const kind = dots(segment);
    if (kind === 2) {
      kept.pop();
    }
    if (kind !== 0) {
      if (last) {
        kept.push("");
      }
      return;
    }
    kept.push(percentEncode(segment, (code) => !PATH_ENCODED.includes(String.fromCharCode(code))));
  });
  return `/${kept.join("/")}`;
}

const FORBIDDEN_HOST = /[\x00-\x20#%\/:<>?@\[\\\]^|\x7f]/;
const DEFAULT_PORTS: Record<string, string> = { http: "80", https: "443" };

function hostAndPort(scheme: string, written: string): { host: string; port: string } | null {
  let host = written;
  let port = "";
  if (written.startsWith("[")) {
    const close = written.indexOf("]");
    if (close < 0) {
      return null;
    }
    host = asciiLower(written.slice(0, close + 1));
    const after = written.slice(close + 1);
    if (after !== "" && !after.startsWith(":")) {
      return null;
    }
    port = after.slice(1);
  } else {
    const colon = written.indexOf(":");
    if (colon >= 0) {
      host = written.slice(0, colon);
      port = written.slice(colon + 1);
    }
    host = asciiLower(percentDecode(host));
    if (host === "" || FORBIDDEN_HOST.test(host)) {
      return null;
    }
  }
  if (port !== "" && (!/^[0-9]+$/.test(port) || Number(port) > 65535)) {
    return null;
  }
  port = port === "" ? "" : String(Number(port));
  return { host, port: port === DEFAULT_PORTS[scheme] ? "" : port };
}

function withAuthority(scheme: string, rest: string): Url | null {
  const start = scan(rest, 0, slashless);
  const tail = rest.slice(start);
  const found = scan(tail, 0, authorityEnd);
  const end = found === tail.length ? -1 : found;
  const authority = end < 0 ? tail : tail.slice(0, end);
  const at = authority.lastIndexOf("@");
  const user = at < 0 ? "" : authority.slice(0, at);
  const place = hostAndPort(scheme, at < 0 ? authority : authority.slice(at + 1));
  if (place === null) {
    return null;
  }
  const pieces = splitTail(end < 0 ? "" : tail.slice(end));
  const base: Url = { scheme, web: true, user, ...place, path: normalPath(pieces.path === "" ? "/" : pieces.path), query: null, fragment: null };
  return encodedTail(base, pieces.query, pieces.fragment);
}

function relative(base: Url, text: string): Url | null {
  if (text.length >= 2 && !slashless(text.charCodeAt(0)) && !slashless(text.charCodeAt(1))) {
    return withAuthority(base.scheme, text);
  }
  const pieces = splitTail(text);
  if (text !== "" && !slashless(text.charCodeAt(0))) {
    return encodedTail({ ...base, path: normalPath(pieces.path) }, pieces.query, pieces.fragment);
  }
  if (pieces.path === "") {
    const query = pieces.query === null ? base.query : null;
    const tail = encodedTail({ ...base, query: null }, pieces.query, pieces.fragment);
    return pieces.query === null ? { ...tail, query } : tail;
  }
  const directory = base.path.slice(0, base.path.lastIndexOf("/") + 1);
  return encodedTail({ ...base, path: normalPath(directory + pieces.path) }, pieces.query, pieces.fragment);
}

function schemeChar(code: number) {
  const lower = code | 32;
  return (lower >= 97 && lower <= 122) || (code >= 48 && code <= 57) || code === 43 || code === 46 || code === 45;
}

function parseUrl(input: string, base: Url | null): Url | null {
  const text = cleanedUrl(input);
  const lead = text.charCodeAt(0) | 32;
  const written = lead >= 97 && lead <= 122 ? scan(text, 1, (code) => !schemeChar(code)) : 0;
  if (written === 0 || text[written] !== ":") {
    return base === null ? null : relative(base, text);
  }
  const name = asciiLower(text.slice(0, written));
  const rest = text.slice(written + 1);
  if (name !== "http" && name !== "https") {
    return { scheme: name, web: false, user: "", host: "", port: "", path: rest, query: null, fragment: null };
  }
  if (base !== null && base.scheme === name && (rest === "" || slashless(rest.charCodeAt(0)))) {
    return relative(base, rest);
  }
  return withAuthority(name, rest);
}

function origin(url: Url) {
  return url.web ? `${url.scheme}://${url.host}${url.port === "" ? "" : `:${url.port}`}` : "null";
}

function fromPath(url: Url) {
  return `${url.path}${url.query === null ? "" : `?${url.query}`}${url.fragment === null ? "" : `#${url.fragment}`}`;
}

function serialized(url: Url) {
  return `${origin(url).replace("://", `://${url.user === "" ? "" : `${url.user}@`}`)}${fromPath(url)}`;
}

class Links {
  page: Url | null;
  shown: string[] = [];
  known = new Map<string, number>();

  constructor(page: string) {
    this.page = parseUrl(page, null);
  }

  number(href: string) {
    const shown = this.shownForm(href);
    if (shown === null) {
      return null;
    }
    let at = this.known.get(shown);
    if (at === undefined) {
      this.shown.push(shown);
      at = this.shown.length - 1;
      this.known.set(shown, at);
    }
    return at + 1;
  }

  list() {
    if (this.shown.length === 0) {
      return "";
    }
    const header =
      this.page !== null && this.shown.some((shown) => shown.startsWith("/"))
        ? `Links (paths are on ${origin(this.page)}):`
        : "Links:";
    return `${header}\n${this.shown.map((shown, at) => `[${at + 1}] ${shown}`).join("\n")}`;
  }

  absolute(href: string) {
    const target = this.resolved(href);
    return target === null ? null : serialized(target);
  }

  resolved(href: string): Url | null {
    const trimmed = trim(href);
    const lower = asciiLower(trimmed);
    if (trimmed === "" || trimmed.startsWith("#")) {
      return null;
    }
    if (["javascript:", "mailto:", "tel:", "data:"].some((scheme) => lower.startsWith(scheme))) {
      return null;
    }
    const target = parseUrl(trimmed, this.page);
    return target !== null && target.web ? target : null;
  }

  shownForm(href: string) {
    const target = this.resolved(href);
    if (target === null) {
      return null;
    }
    const kept = formPairs(target.query ?? "").filter(([key]) => !key.startsWith("utm_"));
    const query = kept.length === 0 ? null : kept.map(([key, value]) => `${formEncode(key)}=${formEncode(value)}`).join("&");
    const bare = { ...target, query, fragment: null };
    const sameSite = this.page !== null && this.page.web && origin(this.page) === origin(bare);
    const shown = sameSite ? fromPath(bare) : serialized(bare);
    return utf8Length(shown) <= LONGEST ? shown : null;
  }
}

type Task = Node | (() => void);

class Writer {
  out: string[] = [];
  links: Links;
  repeated: Set<string>;
  compact = false;
  addresses = true;
  reading: Reading;

  constructor(links: Links, repeated: Set<string>, reading: Reading) {
    this.links = links;
    this.repeated = repeated;
    this.reading = reading;
  }

  take() {
    const taken = this.out.join("");
    this.out = [];
    return taken;
  }

  walk(root: Element, skip: Element | null) {
    const tasks: Task[] = [root];
    while (tasks.length > 0) {
      const task = tasks.pop() as Task;
      if (typeof task === "string") {
        this.out.push(task);
      } else if (typeof task === "function") {
        task();
      } else {
        this.enter(task, skip, tasks);
      }
    }
  }

  enter(element: Element, skip: Element | null, tasks: Task[]) {
    const name = element.name;
    const cleanup = this.reading.cleanup;
    const dropped = cleanup ? DROPPED.has(name) || hidden(element) : NEVER_TEXT.has(name);
    if (skip === element || dropped) {
      return;
    }
    const run = this.compact ? linkRun(element) : null;
    if (run !== null) {
      this.run(run);
    } else if (/^h[1-6]$/.test(name)) {
      this.out.push(`\n\n${"#".repeat(Number(name[1]))} ${text(element)}\n\n`);
    } else if (name === "pre") {
      this.out.push("\n\n```\n" + trimAll(verbatim(element), "\n") + "\n```\n\n");
    } else if (name === "a") {
      this.link(element);
    } else if (name === "br") {
      this.out.push("\n");
    } else if (name === "li") {
      this.wrapped(element, tasks, "\n- ", "\n");
    } else if (name === "td" || name === "th") {
      this.cell(element, tasks);
    } else if (name === "tr") {
      this.wrapped(element, tasks, "\n", "");
    } else if (FURNITURE.has(name)) {
      const outer = this.compact;
      this.compact = cleanup;
      this.out.push("\n");
      tasks.push(() => {
        this.out.push("\n");
        this.compact = outer;
      });
      this.children(element, tasks);
    } else if (BLOCKS.has(name)) {
      this.wrapped(element, tasks, "\n", "\n");
    } else {
      this.children(element, tasks);
    }
  }

  wrapped(element: Element, tasks: Task[], before: string, after: string) {
    this.out.push(before);
    tasks.push(after);
    this.children(element, tasks);
  }

  cell(cell: Element, tasks: Task[]) {
    const start = this.out.length;
    tasks.push(() => {
      if (trim(this.out.slice(start).join("")) === "") {
        this.out.length = start;
      } else {
        this.out.push(" | ");
      }
    });
    this.children(cell, tasks);
  }

  children(element: Element, tasks: Task[]) {
    for (let at = element.children.length - 1; at >= 0; at--) {
      const child = element.children[at];
      tasks.push(typeof child === "string" ? replaceAll(replaceAll(replaceAll(child, "\n", " "), "\r", " "), "\t", " ") : child);
    }
  }

  link(link: Element) {
    const label = labelOf(link);
    const href = attr(link, "href");
    const footnote = label.startsWith("[") && href !== null && href.startsWith("#");
    if (label === "" || (footnote && this.reading.cleanup) || this.repeated.has(label.toLowerCase())) {
      return;
    }
    this.out.push(this.numbered(link, label));
  }

  numbered(link: Element, label: string) {
    const href = this.addresses ? attr(link, "href") : null;
    if (!this.reading.links) {
      const address = href === null ? null : this.links.absolute(href);
      return address === null ? label : `${label} [${address}]`;
    }
    const number = href === null ? null : this.links.number(href);
    return number === null ? label : `${label} [${number}]`;
  }

  run(run: Element[]) {
    const shown = run.slice(0, SHOWN_OF_A_RUN).map((link) => this.numbered(link, labelOf(link)));
    const more = Math.max(0, run.length - SHOWN_OF_A_RUN);
    this.out.push(`\n${shown.join(" · ")} (+${more} more links)\n`);
  }
}

function labelOf(link: Element) {
  const written = text(link);
  if (written !== "") {
    return written;
  }
  const image = first(link, "img");
  const alt = image === null ? null : attr(image, "alt");
  return trim(attr(link, "aria-label") ?? attr(link, "title") ?? alt ?? "");
}

function verbatim(element: Element): string {
  const out: string[] = [];
  const pending: Node[] = element.children.slice().reverse();
  while (pending.length > 0) {
    const node = pending.pop() as Node;
    if (typeof node === "string") {
      out.push(node);
    } else if (node.name === "br") {
      out.push("\n");
    } else if (!DROPPED.has(node.name)) {
      for (let at = node.children.length - 1; at >= 0; at--) {
        pending.push(node.children[at]);
      }
    }
  }
  return out.join("");
}
function tidy(written: string) {
  const rows: string[] = [];
  let blank = false;
  let code = false;
  for (const raw of lines(written)) {
    const fence = trimStart(raw).startsWith("```");
    const line = code || fence ? trimEnd(raw) : trimEnd(trimEndAll(trimEndAll(squash(raw), " |"), "|"));
    if (fence) {
      code = !code;
    }
    if (line === "" && !code) {
      blank = rows.length > 0;
      continue;
    }
    if (rows.length > 0 && rows[rows.length - 1] === "-") {
      rows[rows.length - 1] = `- ${line}`;
      blank = false;
      continue;
    }
    const last = rows.length > 0 ? rows[rows.length - 1] : null;
    const inList = (line === "-" || line.startsWith("- ")) && last !== null && last.startsWith("- ");
    if (blank && !inList) {
      rows.push("");
    }
    blank = false;
    rows.push(line);
  }
  return trimEnd(rows.join("\n"));
}

function distill(html: string, url: string, reading: Reading) {
  const root = parse(html, false);
  const body = first(root, "body") ?? root;
  const writer = new Writer(new Links(url), reading.cleanup ? repeatedLabels(body) : new Set(), reading);
  const main = reading.mainContent ? mainOf(body) : null;
  let written: string;
  if (main !== null) {
    writer.walk(main, null);
    const lead = tidy(writer.take());
    writer.compact = reading.cleanup;
    writer.addresses = false;
    writer.walk(body, main);
    const rest = tidy(writer.take());
    written = rest === "" ? lead : `${lead}\n\n--- around the main content ---\n${rest}`;
  } else {
    writer.walk(body, null);
    written = tidy(writer.take());
  }
  const titled = first(root, "title");
  const title = titled === null ? "" : text(titled);
  if (title !== "" && !written.startsWith("# ")) {
    written = written === "" ? `# ${title}` : `# ${title}\n\n${written}`;
  }
  const links = writer.links.list();
  return links === "" ? written : `${written}\n\n${links}`;
}

function isFeed(body: string) {
  const head = asciiLower([...trimStart(body).slice(0, 2048)].slice(0, 512).join(""));
  return head.includes("<rss") || head.includes("<feed") || head.includes("<rdf:rdf");
}

function readFeed(body: string, reading: Reading) {
  const root = parse(body, true);
  const channel = first(root, "channel") ?? first(root, "feed") ?? root;
  const titled = elements(channel).find((element) => element.name === "title");
  const entries = find(root, (element) => element.name === "item" || element.name === "entry").map((item) =>
    entry(item, reading),
  );
  const head = titled === undefined ? "" : `# ${text(titled)}\n\n`;
  return trimEnd(head + entries.join("\n"));
}

function entry(item: Element, reading: Reading) {
  const field = (names: string[]) => elements(item).find((element) => names.includes(element.name));
  const titled = field(["title"]);
  const dated = field(["pubdate", "published", "updated", "date"]);
  const linked = elements(item)
    .filter((element) => element.name === "link")
    .find((link) => {
      const rel = attr(link, "rel");
      return rel === null || rel === "alternate";
    });
  const link = linked === undefined ? "" : attr(linked, "href") ?? text(linked);
  const date = dated === undefined ? "" : text(dated);
  let line = `- ${titled === undefined ? "" : text(titled)}`;
  if (date !== "") {
    line += ` (${date})`;
  }
  if (link !== "") {
    line += ` ${link}`;
  }
  const summarized = field(["description", "summary", "encoded", "content"]);
  if (summarized !== undefined) {
    line += `\n  ${summaryOf(summarized, reading)}`;
  }
  return line;
}

function summaryOf(element: Element, reading: Reading) {
  const written = text(element);
  const inline = { ...(reading.configured ?? reading), mainContent: false, links: false };
  const flat = squash(written.includes("<") ? distill(written, "", inline) : written);
  const at = charIndex(flat, SUMMARY);
  return at < 0 ? flat : `${flat.slice(0, at)}…`;
}

type Json = null | boolean | string | { number: string } | Json[] | Map<string, Json>;

type Cursor = { text: string; at: number; depth: number };

function fail(): never {
  throw new Error("not json");
}

function skipBlank(cursor: Cursor) {
  while (cursor.at < cursor.text.length && " \t\n\r".includes(cursor.text[cursor.at])) {
    cursor.at++;
  }
}

function parseJson(text: string): Json | undefined {
  const cursor: Cursor = { text, at: 0, depth: 0 };
  try {
    skipBlank(cursor);
    const value = jsonValue(cursor);
    skipBlank(cursor);
    return cursor.at === text.length ? value : undefined;
  } catch {
    return undefined;
  }
}

function jsonValue(cursor: Cursor): Json {
  const char = cursor.text[cursor.at];
  if (char === "{" || char === "[") {
    cursor.depth++;
    if (cursor.depth >= JSON_DEPTH) {
      fail();
    }
    const value = char === "{" ? jsonObject(cursor) : jsonArray(cursor);
    cursor.depth--;
    return value;
  }
  if (char === '"') {
    return jsonString(cursor);
  }
  for (const [word, value] of [["true", true], ["false", false], ["null", null]] as [string, Json][]) {
    if (cursor.text.startsWith(word, cursor.at)) {
      cursor.at += word.length;
      return value;
    }
  }
  const pattern = /-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+\-]?[0-9]+)?/y;
  pattern.lastIndex = cursor.at;
  const found = pattern.exec(cursor.text);
  if (found === null) {
    fail();
  }
  cursor.at += found[0].length;
  formatNumber(found[0]);
  return { number: found[0] };
}

function jsonList<T>(cursor: Cursor, end: string, item: () => T): T[] {
  cursor.at++;
  skipBlank(cursor);
  const items: T[] = [];
  if (cursor.text[cursor.at] === end) {
    cursor.at++;
    return items;
  }
  for (;;) {
    skipBlank(cursor);
    items.push(item());
    skipBlank(cursor);
    const next = cursor.text[cursor.at++];
    if (next === end) {
      return items;
    }
    if (next !== ",") {
      fail();
    }
  }
}

function jsonArray(cursor: Cursor): Json[] {
  return jsonList(cursor, "]", () => jsonValue(cursor));
}

function jsonObject(cursor: Cursor): Map<string, Json> {
  const fields = new Map<string, Json>();
  const pairs = jsonList(cursor, "}", () => {
    if (cursor.text[cursor.at] !== '"') {
      fail();
    }
    const key = jsonString(cursor);
    skipBlank(cursor);
    if (cursor.text[cursor.at++] !== ":") {
      fail();
    }
    skipBlank(cursor);
    return [key, jsonValue(cursor)] as [string, Json];
  });
  for (const [key, value] of pairs) {
    fields.set(key, value);
  }
  return fields;
}

const ESCAPES: Record<string, string> = { '"': '"', "\\": "\\", "/": "/", b: "\b", f: "\f", n: "\n", r: "\r", t: "\t" };

function hexUnit(cursor: Cursor) {
  const digits = cursor.text.slice(cursor.at, cursor.at + 4);
  if (!/^[0-9a-fA-F]{4}$/.test(digits)) {
    fail();
  }
  cursor.at += 4;
  return parseInt(digits, 16);
}

function escaped(cursor: Cursor) {
  const mark = cursor.text[cursor.at++];
  if (mark !== "u") {
    return ESCAPES[mark] ?? fail();
  }
  const unit = hexUnit(cursor);
  if (unit >= 0xdc00 && unit <= 0xdfff) {
    fail();
  }
  if (unit < 0xd800 || unit > 0xdbff) {
    return String.fromCharCode(unit);
  }
  if (!cursor.text.startsWith("\\u", cursor.at)) {
    fail();
  }
  cursor.at += 2;
  const low = hexUnit(cursor);
  if (low < 0xdc00 || low > 0xdfff) {
    fail();
  }
  return String.fromCharCode(unit, low);
}

function jsonString(cursor: Cursor) {
  cursor.at++;
  const out: string[] = [];
  for (;;) {
    const found = scan(cursor.text, cursor.at, (code) => code === 34 || code === 92 || code < 0x20);
    if (found === cursor.text.length) {
      fail();
    }
    out.push(cursor.text.slice(cursor.at, found));
    cursor.at = found;
    const char = cursor.text[cursor.at++];
    if (char === '"') {
      return out.join("");
    }
    if (char !== "\\") {
      fail();
    }
    out.push(escaped(cursor));
  }
}

const I64_LEAST = BigInt("-9223372036854775808");
const U64_MOST = BigInt("18446744073709551615");

function formatNumber(written: string) {
  if (written === "-0") {
    return "-0.0";
  }
  if (!/[.eE]/.test(written)) {
    const whole = BigInt(written);
    if (whole >= I64_LEAST && whole <= U64_MOST) {
      return whole.toString();
    }
  }
  const value = Number(written);
  if (!Number.isFinite(value)) {
    fail();
  }
  return shortest(value);
}

function shortest(value: number) {
  if (value === 0) {
    return Object.is(value, -0) ? "-0.0" : "0.0";
  }
  const sign = value < 0 ? "-" : "";
  const [mantissa, power] = Math.abs(value).toExponential().split("e");
  const digits = mantissa.replace(".", "");
  const length = digits.length;
  const kk = Number(power) + 1;
  const k = kk - length;
  if (k >= 0 && kk <= 16) {
    return `${sign}${digits}${"0".repeat(k)}.0`;
  }
  if (kk > 0 && kk <= 16) {
    return `${sign}${digits.slice(0, kk)}.${digits.slice(kk)}`;
  }
  if (kk > -5 && kk <= 0) {
    return `${sign}0.${"0".repeat(-kk)}${digits}`;
  }
  const exponent = kk - 1 < 0 ? `${kk - 1}` : `+${kk - 1}`;
  if (length === 1) {
    return `${sign}${digits}e${exponent}`;
  }
  return `${sign}${digits[0]}.${digits.slice(1)}e${exponent}`;
}

function isObject(value: Json): value is Map<string, Json> {
  return value instanceof Map;
}

function sortedKeys(fields: Map<string, Json>) {
  return [...fields.keys()].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

function serialize(value: Json): string {
  if (value === null) {
    return "null";
  }
  if (typeof value === "boolean") {
    return String(value);
  }
  if (typeof value === "string") {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map(serialize).join(",")}]`;
  }
  if (isObject(value)) {
    const fields = sortedKeys(value).filter((key) => value.get(key) !== null);
    return `{${fields.map((key) => `${JSON.stringify(key)}:${serialize(value.get(key) as Json)}`).join(",")}}`;
  }
  return formatNumber(value.number);
}

function scalar(value: Json): string | null {
  if (value === null || isObject(value)) {
    return null;
  }
  if (typeof value === "string") {
    return trim(value) === "" ? null : value;
  }
  if (typeof value === "boolean") {
    return String(value);
  }
  if (Array.isArray(value)) {
    if (value.length === 0) {
      return null;
    }
    if (value.every((item) => !isObject(item) && !Array.isArray(item))) {
      return value.map(scalar).filter((text) => text !== null).join(", ");
    }
    return `[${value.length} items]`;
  }
  return formatNumber(value.number);
}

function records(value: Json): { items: Json[] | null; meta: [string, string][] } {
  const objects = (list: Json) => Array.isArray(list) && list.length > 1 && list.every(isObject);
  if (Array.isArray(value) && objects(value)) {
    return { items: value, meta: [] };
  }
  if (!isObject(value)) {
    return { items: null, meta: [] };
  }
  let main: string | null = null;
  for (const key of sortedKeys(value)) {
    const list = value.get(key) as Json;
    if (objects(list) && (main === null || (list as Json[]).length >= (value.get(main) as Json[]).length)) {
      main = key;
    }
  }
  if (main === null) {
    return { items: null, meta: [] };
  }
  const meta: [string, string][] = [];
  for (const key of sortedKeys(value)) {
    const text = key === main ? null : scalar(value.get(key) as Json);
    if (text !== null) {
      meta.push([key, text]);
    }
  }
  return { items: value.get(main) as Json[], meta };
}

function flatten(item: Json): [string, string][] {
  const out: [string, string][] = [];
  if (!isObject(item)) {
    return out;
  }
  for (const key of sortedKeys(item)) {
    const value = item.get(key) as Json;
    if (isObject(value)) {
      for (const name of sortedKeys(value)) {
        const text = scalar(value.get(name) as Json);
        if (text !== null) {
          out.push([`${key}.${name}`, text]);
        }
      }
    } else {
      const text = scalar(value);
      if (text !== null) {
        out.push([key, text]);
      }
    }
  }
  return out;
}

function pairs(fields: [string, string][]) {
  return fields.map(([key, value]) => `${key}=${value}`).join("; ");
}

function compactJson(body: string) {
  const value = parseJson(trim(body));
  if (value === undefined) {
    return null;
  }
  const { items, meta } = records(value);
  if (items === null) {
    return serialize(value);
  }
  const rows = items.map(flatten);
  const has = (row: [string, string][], [key, value]: [string, string]) =>
    row.some(([known, text]) => known === key && text === value);
  const shared = rows.length >= 2 ? rows[0].filter((field) => rows.every((row) => has(row, field))) : [];
  const out = meta.map(([key, value]) => `${key}: ${value}`);
  if (shared.length > 0) {
    out.push(`every item: ${pairs(shared)}`);
  }
  rows.forEach((row, at) => {
    out.push(`[${at + 1}] ${pairs(row.filter((field) => !has(shared, field)))}`);
  });
  return out.join("\n");
}

type Kind = "html" | "json" | "feed" | "text";

const MARKS = ["<!doctype html", "<html", "<head", "<body", "<div", "<p>", "<p ", "<br", "<a href", "<h1", "<title"];

function kindOf(body: string, contentType: string | null, formats: boolean): Kind {
  const declared = asciiLower(contentType ?? "");
  const start = trimStart(body);
  const json = declared.includes("json") || (declared === "" && (start.startsWith("{") || start.startsWith("[")));
  if (formats && json) {
    return "json";
  }
  if (formats && isFeed(body) && !declared.includes("html")) {
    return "feed";
  }
  if (json) {
    return "text";
  }
  if (declared.includes("html")) {
    return "html";
  }
  if (declared !== "" && (!formats || !declared.includes("xml"))) {
    return "text";
  }
  const head = asciiLower([...start.slice(0, 4096)].slice(0, 1024).join(""));
  return MARKS.some((mark) => head.includes(mark)) ? "html" : "text";
}

function readPage(body: string, contentType: string | null, url: string, reading: Reading): { text: string; error?: string } {
  switch (kindOf(body, contentType, reading.formats)) {
    case "text":
      return { text: trim(body) };
    case "json":
      return { text: compactJson(body) ?? trim(body) };
    case "feed":
      return { text: readFeed(body, reading) };
    default:
      break;
  }
  const written = distill(body, url, reading);
  const shown = utf8Length(trim(written.split("\n\nLinks")[0]));
  const markup = utf8Length(body);
  const thin = shown === 0 || (shown < THIN_TEXT && markup > THIN_HTML);
  if (thin && reading.emptyPageError) {
    return {
      text: "",
      error:
        `${url} has no readable text in its HTML (${shown} characters of text in ${markup} of markup): it is ` +
        "probably built by JavaScript, or behind a consent wall, a login or a bot check.",
    };
  }
  return { text: written };
}

function clipped(written: string, limit: number) {
  if (charCount(written) <= limit) {
    return written;
  }
  const split = written.lastIndexOf("\n\nLinks");
  if (split < 0) {
    return clip(written, limit);
  }
  const body = written.slice(0, split);
  const list = written.slice(split + 2);
  const floor = Math.floor(limit / 4);
  let budget = limit;
  for (;;) {
    const cut = clip(body, budget);
    const kept = referenced(cut, list);
    const overflow = Math.max(0, charCount(cut) + charCount(kept) + 2 - limit);
    if (overflow === 0 || budget <= floor) {
      return kept === "" ? cut : `${cut}\n\n${kept}`;
    }
    budget = Math.max(Math.max(0, budget - overflow), floor);
  }
}

function referenced(body: string, list: string) {
  let highest = 0;
  for (const found of body.matchAll(/\[(\d+)\]/g)) {
    const number = Number(found[1]);
    if (Number.isSafeInteger(number) && number > highest) {
      highest = number;
    }
  }
  if (highest === 0) {
    return "";
  }
  return lines(list).slice(0, highest + 1).join("\n");
}
