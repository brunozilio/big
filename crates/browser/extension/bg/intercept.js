import { watching } from "./capture.js";
import { isAttached, onAttach, onEvent, send } from "./cdp.js";
import { redact } from "./logs.js";
import { clip, fromBase64, refuse, toBase64 } from "./results.js";
import { hostOf } from "./sites.js";

const MOST_RULES = 50;
const MOST_RECENT = 50;
const REASONS = [
  "Failed",
  "Aborted",
  "TimedOut",
  "AccessDenied",
  "ConnectionClosed",
  "ConnectionReset",
  "ConnectionRefused",
  "ConnectionAborted",
  "ConnectionFailed",
  "NameNotResolved",
  "InternetDisconnected",
  "AddressUnreachable",
  "BlockedByClient",
  "BlockedByResponse",
];
const rules = new Map();
const tops = new Map();
const recent = new Map();
let counter = 0;

const escaped = (char) => char.replace(/[.*+?^${}()|[\]\\/]/g, "\\$&");
const encoded = (value) => toBase64(new TextEncoder().encode(String(value ?? "")));
const isObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);

function wildcard(pattern) {
  let source = "";
  for (let at = 0; at < pattern.length; at += 1) {
    const char = pattern[at];
    if (char === "\\" && at + 1 < pattern.length) {
      at += 1;
      source += escaped(pattern[at]);
    } else if (char === "*") source += ".*";
    else if (char === "?") source += ".";
    else source += escaped(char);
  }
  return new RegExp(`^${source}$`);
}

function headerList(base, changes) {
  const changed = new Map(Object.entries(changes || {}).map(([name, value]) => [name.toLowerCase(), { name, value }]));
  const kept = base.filter((header) => !changed.has(header.name.toLowerCase()));
  const added = [...changed.values()].filter((header) => header.value !== null && header.value !== undefined);
  return [...kept, ...added.map((header) => ({ name: header.name, value: String(header.value) }))];
}

function defined(params) {
  return Object.fromEntries(Object.entries(params).filter(([, value]) => value !== undefined));
}

function headersShape(value, name) {
  if (value === undefined) return;
  if (!isObject(value)) refuse(`\`${name}.headers\` must be an object of header name to value (null removes the header).`);
}

function replacements(value) {
  if (value === undefined) return;
  if (!Array.isArray(value)) refuse("`response.replace` must be a list of {find, with, regex?} items.");
  for (const step of value) {
    if (!isObject(step) || typeof step.find !== "string" || !step.find) refuse("Each `response.replace` item needs a non-empty `find` string.");
    if (step.regex) {
      try {
        new RegExp(step.find, "g");
      } catch (error) {
        refuse(`\`${step.find}\` is not a valid regular expression: ${error.message}`);
      }
    }
  }
}

function effectOf(input, stage) {
  const given = ["request", "respond", "block"].filter((key) => input[key] !== undefined && input[key] !== false);
  if (stage === "request" && given.length !== 1) {
    refuse("At the request stage give exactly one of `request` (change it), `respond` (answer without the network) or `block`.");
  }
  if (stage === "response" && (given.length > 0 || !isObject(input.response))) {
    refuse("At the response stage give only `response` (status, headers, body or replace).");
  }
  if (input.block !== undefined && input.block !== true && !REASONS.includes(input.block)) {
    refuse(`\`block\` must be true or one of: ${REASONS.join(", ")}.`);
  }
  for (const key of ["request", "respond", "response"]) {
    if (input[key] !== undefined && !isObject(input[key])) refuse(`\`${key}\` must be an object.`);
    headersShape(input[key]?.headers, key);
  }
  replacements(input.response?.replace);
}

function status(value, name) {
  if (value === undefined) return undefined;
  const code = Number(value);
  if (!Number.isInteger(code) || code < 100 || code > 599) refuse(`\`${name}.status\` must be an HTTP status between 100 and 599.`);
  return code;
}

function built(input, tab) {
  const urlPattern = String(input.urlPattern || "").trim();
  if (!urlPattern) refuse("`urlPattern` is required: a URL wildcard such as `*://*/api/users*` (`*` any text, `?` one character).");
  const stage = input.stage === undefined ? "request" : input.stage;
  if (stage !== "request" && stage !== "response") refuse("`stage` must be 'request' or 'response'.");
  effectOf(input, stage);
  const times = input.times === undefined ? 0 : Number(input.times);
  if (!Number.isInteger(times) || times < 0) refuse("`times` must be a whole number (0 or absent means until removed).");
  const respond = input.respond && { ...input.respond, status: status(input.respond.status, "respond") ?? 200 };
  const response = input.response && { ...input.response, status: status(input.response.status, "response") };
  return {
    id: `r${(counter += 1)}`,
    urlPattern,
    matcher: wildcard(urlPattern),
    stage,
    method: String(input.method || "").toUpperCase(),
    resourceType: String(input.resourceType || "").toLowerCase(),
    host: hostOf(tab.url || ""),
    request: input.request,
    respond,
    block: input.block,
    response,
    times,
    hits: 0,
  };
}

function effectText(rule) {
  if (rule.block) return `block (${rule.block === true ? "BlockedByClient" : rule.block})`;
  if (rule.respond) return `answer ${rule.respond.status} without the network (${String(rule.respond.body ?? "").length} chars)`;
  const change = rule.request || rule.response;
  const parts = [];
  if (change.url) parts.push(`url → ${clip(change.url, 120)}`);
  if (change.method) parts.push(`method → ${change.method}`);
  if (change.status) parts.push(`status → ${change.status}`);
  if (change.headers) parts.push(`headers ${Object.keys(change.headers).join(", ")}`);
  if (change.body !== undefined) parts.push(`body replaced (${String(change.body).length} chars)`);
  if (change.replace) parts.push(`${change.replace.length} text replacement(s)`);
  return `change ${rule.stage}: ${parts.join("; ") || "nothing"}`;
}

function described(rule) {
  const filters = [rule.method, rule.resourceType].filter(Boolean).join(" ");
  const limit = rule.times ? `, ${rule.hits}/${rule.times} uses` : `, ${rule.hits} hit${rule.hits === 1 ? "" : "s"}`;
  return `${rule.id} [${rule.stage}] ${filters ? `${filters} ` : ""}${rule.urlPattern} → ${effectText(rule)} (on ${rule.host}${limit})`;
}

async function apply(tabId) {
  const list = rules.get(tabId) || [];
  if (list.length === 0) {
    if (isAttached(tabId)) await send(tabId, "Fetch.disable").catch(() => {});
    return;
  }
  const seen = new Set();
  const patterns = [];
  for (const rule of list) {
    const key = `${rule.stage} ${rule.urlPattern}`;
    if (seen.has(key)) continue;
    seen.add(key);
    patterns.push({ urlPattern: rule.urlPattern, requestStage: rule.stage === "response" ? "Response" : "Request" });
  }
  await send(tabId, "Fetch.enable", { patterns });
}

function matching(tabId, params, stage) {
  const top = tops.get(tabId);
  const url = params.request?.url || "";
  const type = String(params.resourceType || "").toLowerCase();
  return (rules.get(tabId) || []).find(
    (rule) =>
      rule.stage === stage &&
      (!top || top === rule.host) &&
      rule.matcher.test(url) &&
      (!rule.method || rule.method === params.request?.method) &&
      (!rule.resourceType || rule.resourceType === type),
  );
}

function resume(tabId, requestId) {
  return send(tabId, "Fetch.continueRequest", { requestId }).catch(() => {});
}

async function onRequest(tabId, rule, params) {
  const requestId = params.requestId;
  if (rule.block) {
    await send(tabId, "Fetch.failRequest", { requestId, errorReason: rule.block === true ? "BlockedByClient" : rule.block });
    return "blocked";
  }
  if (rule.respond) {
    const answer = rule.respond;
    await send(tabId, "Fetch.fulfillRequest", { requestId, responseCode: answer.status, responseHeaders: headerList([], answer.headers), body: encoded(answer.body) });
    return `answered ${answer.status}`;
  }
  const change = rule.request;
  const original = Object.entries(params.request?.headers || {}).map(([name, value]) => ({ name, value: String(value) }));
  await send(
    tabId,
    "Fetch.continueRequest",
    defined({
      requestId,
      url: change.url,
      method: change.method,
      postData: change.body === undefined ? undefined : encoded(change.body),
      headers: change.headers ? headerList(original, change.headers) : undefined,
    }),
  );
  return "request changed";
}

function rewritten(original, change) {
  if (change.body === undefined && !change.replace) return original.base64Encoded ? original.body : encoded(original.body);
  let text = change.body !== undefined ? String(change.body) : original.base64Encoded ? new TextDecoder().decode(fromBase64(original.body)) : original.body;
  for (const step of change.replace || []) {
    const swap = String(step.with ?? "");
    text = step.regex ? text.replace(new RegExp(step.find, "g"), swap) : text.split(step.find).join(swap);
  }
  return encoded(text);
}

async function onResponse(tabId, rule, params) {
  const requestId = params.requestId;
  const code = params.responseStatusCode;
  const change = rule.response;
  const original = await send(tabId, "Fetch.getResponseBody", { requestId });
  const changed = headerList(params.responseHeaders || [], change.headers);
  const responseHeaders = headerList(changed, { "content-length": null, "content-encoding": null });
  await send(tabId, "Fetch.fulfillRequest", { requestId, responseCode: change.status || code, responseHeaders, body: rewritten(original, change) });
  return `response changed (${change.status || code})`;
}

function claimed(tabId, rule) {
  rule.hits += 1;
  if (rule.times && rule.hits >= rule.times) {
    rules.set(tabId, (rules.get(tabId) || []).filter((kept) => kept !== rule));
    apply(tabId).catch(() => {});
  }
}

function logged(tabId, rule, params, outcome) {
  const log = recent.get(tabId) || [];
  log.push({ at: new Date().toISOString().slice(11, 23), rule: rule.id, method: params.request?.method || "", url: redact(params.request?.url || ""), outcome });
  if (log.length > MOST_RECENT) log.splice(0, log.length - MOST_RECENT);
  recent.set(tabId, log);
}

async function paused(tabId, params) {
  const atResponse = params.responseStatusCode !== undefined || params.responseErrorReason !== undefined;
  const code = params.responseStatusCode;
  const passing = params.responseErrorReason !== undefined || (code >= 300 && code < 400);
  const rule = passing ? null : matching(tabId, params, atResponse ? "response" : "request");
  if (!rule) return resume(tabId, params.requestId);
  claimed(tabId, rule);
  const outcome = atResponse ? await onResponse(tabId, rule, params) : await onRequest(tabId, rule, params);
  logged(tabId, rule, params, outcome);
}

onEvent((tabId, method, params) => {
  if (method === "Page.frameNavigated" && !params.frame?.parentId) tops.set(tabId, hostOf(params.frame?.url || ""));
  if (method === "Fetch.requestPaused" && !watching(tabId)) paused(tabId, params).catch(() => resume(tabId, params.requestId));
});

onAttach((tabId) => ((rules.get(tabId) || []).length > 0 ? apply(tabId) : undefined));

chrome.tabs.onRemoved.addListener((tabId) => {
  rules.delete(tabId);
  tops.delete(tabId);
  recent.delete(tabId);
});

async function add(tab, input) {
  if (!hostOf(tab.url || "")) refuse("Interception works only on http(s) pages; navigate the tab to the site first.");
  const list = rules.get(tab.id) || [];
  if (list.length >= MOST_RULES) refuse(`Tab ${tab.id} already has ${MOST_RULES} rules; remove some first.`);
  const rule = built(input, tab);
  rules.set(tab.id, [...list, rule]);
  if (!tops.has(tab.id)) tops.set(tab.id, rule.host);
  try {
    await apply(tab.id);
  } catch (error) {
    rules.set(tab.id, list);
    refuse(`Chrome refused the interception: ${error?.message || error}`);
  }
  return `Added ${described(rule)}.\nIt acts on requests from tab ${tab.id} from now on, while the tab stays on ${rule.host}; reload the page if the requests you want already happened.`;
}

function listed(tab) {
  const list = rules.get(tab.id) || [];
  const log = recent.get(tab.id) || [];
  const lines = [list.length ? `Rules on tab ${tab.id}:\n${list.map(described).join("\n")}` : `No interception rules on tab ${tab.id}.`];
  if (list.length && !isAttached(tab.id)) lines.push("The browser tools are not attached to this tab right now; the rules come back into effect on the next browser call.");
  if (log.length) lines.push(`Recent interceptions:\n${log.slice(-20).map((entry) => `[${entry.at}] ${entry.rule} ${entry.method} ${clip(entry.url, 300)} → ${entry.outcome}`).join("\n")}`);
  return lines.join("\n\n");
}

async function removed(tab, ruleId) {
  const list = rules.get(tab.id) || [];
  if (!list.some((rule) => rule.id === ruleId)) refuse(`Tab ${tab.id} has no rule \`${ruleId}\`; use action 'list' to see its rules.`);
  rules.set(tab.id, list.filter((rule) => rule.id !== ruleId));
  await apply(tab.id);
  return `Removed rule ${ruleId}.`;
}

async function cleared(tab) {
  const count = (rules.get(tab.id) || []).length;
  rules.delete(tab.id);
  recent.delete(tab.id);
  await apply(tab.id);
  return `Removed ${count} rule${count === 1 ? "" : "s"} from tab ${tab.id}; its requests go through untouched again.`;
}

export async function intercept(tab, input) {
  if (input.action === "add") return add(tab, input);
  if (input.action === "list") return listed(tab);
  if (input.action === "remove") return removed(tab, input.ruleId);
  if (input.action === "clear") return cleared(tab);
  refuse("`action` must be 'add', 'list', 'remove' or 'clear'.");
}
