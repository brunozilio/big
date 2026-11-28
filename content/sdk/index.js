const IDLE_TICKS = 1000;
const MOST_TICKS = 1000000;
const ROOT = "main";

const registry = { workflows: new Map(), steps: new Map() };

export class EffectError extends Error {
  constructor(kind, message, data, seq) {
    super(message || kind);
    this.name = "EffectError";
    this.kind = kind;
    this.data = data === undefined ? null : data;
    this.seq = seq;
  }
}

export class StepFailed extends Error {
  constructor(step, outcome) {
    super(`step ${step} failed`);
    this.name = "StepFailed";
    this.step = step;
    this.outcome = outcome;
  }
}

export class Cancelled extends Error {
  constructor(reason) {
    super(reason || "cancelled");
    this.name = "Cancelled";
    this.reason = reason || null;
  }
}

class Nondeterminism extends Error {
  constructor(message, seq) {
    super(message);
    this.name = "Nondeterminism";
    this.seq = seq;
  }
}

const block = (type, fields) => Object.freeze({ type, ...fields });

export const chat = (session, options) => block("chat", { session, composer: Boolean(options && options.composer) });
export const issue = (ref) => block("issue", { ref });
const changeOf = (ref, options) => ({ change: ref, capability: (options && options.capability) || "code" });
export const change = (ref, options) => block("change", changeOf(ref, options));
export const review = (ref, options) =>
  block("review", options && options.session ? { ...changeOf(ref, options), session: options.session } : changeOf(ref, options));
export const pipeline = (run) => block("pipeline", { run });
export const merge = (ref, methods, options) => block("merge", { ...changeOf(ref, options), methods: methods || [] });
export const findingList = (fields) => block("findings", fields || {});
export const qualityReport = (fields) => block("quality", fields || {});
export const markdown = (text) => block("markdown", { text: String(text === undefined ? "" : text) });
export const testCases = (list, options) =>
  block("cases", {
    cases: Array.isArray(list) ? list : [],
    session: (options && options.session) || null,
    approve: (options && options.approve) || null,
    changes: (options && options.changes) || null,
  });
export const logs = (input) =>
  Array.isArray(input) ? block("logs", { commands: input }) : block("logs", { text: String(input === undefined ? "" : input) });
export const form = (fields, options) => {
  const chosen = options || {};
  const extra = {};
  if (chosen.title) extra.title = String(chosen.title);
  if (chosen.submit) extra.submit = String(chosen.submit);
  return block("form", { fields: fields || [], ...extra });
};
export const stages = (list) => block("stages", { stages: Array.isArray(list) ? list : [] });
export const members = (list, options) =>
  block("members", { items: Array.isArray(list) ? list : [], removable: Boolean(options && options.removable) });
export const tasks = (list, options) => {
  const chosen = options || {};
  return block("tasks", {
    tasks: Array.isArray(list) ? list : [],
    kinds: chosen.kinds || [],
    locked: Boolean(chosen.locked),
    environment: chosen.environment || null,
    owners: Array.isArray(chosen.owners) ? chosen.owners : [],
  });
};
export const actions = (buttons) =>
  block("actions", {
    buttons: (buttons || []).map((button) => (typeof button === "string" ? { id: button, label: button } : button)),
  });

const blocks = (view) => (view === undefined || view === null ? [] : Array.isArray(view) ? view : [view]);

const named = (kind, called) => (name) => {
  const text = String(name === undefined || name === null ? "" : name).trim();
  if (!text) throw new TypeError(`${called ?? kind}() needs a name`);
  return Object.freeze({ [kind]: text });
};
export const role = named("role");
export const person = named("person");
export const inGroup = named("group", "inGroup");
export const admins = () => "admins";

const assigneeOf = (value) => {
  if (value === "admins") return value;
  if (value === null || typeof value !== "object") return undefined;
  const kind = ["role", "person", "group"].find((key) => typeof value[key] === "string" && value[key].trim());
  return kind ? { [kind]: value[kind].trim() } : undefined;
};

export const TRIGGER_SCHEMA = 1;

export const on = (kind, filter, options) =>
  Object.freeze({ schema_version: TRIGGER_SCHEMA, ...(options || {}), type: "trigger", kind, filter: filter || {} });

const outcome = (kind, fields) => Object.freeze({ type: "outcome", outcome: kind, ...fields });
const isOutcome = (value) => value !== null && typeof value === "object" && value.type === "outcome";

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function hash(text) {
  let h = 2166136261;
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  return h;
}

function generator(seed) {
  let a = hash(`a${seed}`);
  let b = hash(`b${seed}`);
  let c = hash(`c${seed}`);
  let d = hash(`d${seed}`);
  return () => {
    const t = (((a + b) >>> 0) + d) >>> 0;
    d = (d + 1) >>> 0;
    a = b ^ (b >>> 9);
    b = (c + (c << 3)) >>> 0;
    c = ((c << 21) | (c >>> 11)) >>> 0;
    c = (c + t) >>> 0;
    return t / 4294967296;
  };
}

class Runtime {
  constructor(activation) {
    this.seq = 0;
    this.activity = 0;
    this.commands = new Map();
    this.signals = [];
    this.cancelled = new Set();
    this.dropped = new Set();
    this.attempts = new Map();
    this.outputs = new Map();
    this.gotos = new Map();
    this.tries = new Map();
    this.groups = new Map();
    this.item = Object.freeze({ ...(activation.item || {}) });
    this.application = activation.application || null;
    this.seen = activation.seen && typeof activation.seen === "object" ? activation.seen : {};
    this.clock = Number(activation.start || 0);
    this.random = generator(String(activation.seed === undefined ? "" : activation.seed));
    this.finished = null;
    this.workflow = null;
  }

  next() {
    this.activity += 1;
    const seq = this.seq;
    this.seq += 1;
    return seq;
  }

  issue(scope, kind, args, options) {
    const seq = this.next();
    const waiting = deferred();
    const command = { seq, kind, branch: scope.branch, step: scope.step, attempt: scope.attempt, args };
    for (const key of ["connection", "retry", "timeout", "optional", "operation"]) {
      if (options && options[key] !== undefined) command[key] = options[key];
    }
    this.commands.set(seq, { command, waiting, resolved: false });
    const promise = waiting.promise;
    promise.seq = seq;
    promise.kind = kind;
    return promise;
  }

  signal(scope, kind, data) {
    const seq = this.next();
    this.signals.push({ seq, kind, branch: scope.branch, step: scope.step, attempt: scope.attempt, ...data });
    return seq;
  }

  deliver(entry) {
    const known = this.commands.get(entry.seq);
    if (!known) throw new Nondeterminism(`command ${entry.seq} was not issued in this replay`, entry.seq);
    if (entry.kind !== undefined && entry.kind !== known.command.kind) {
      throw new Nondeterminism(`command ${entry.seq} was ${entry.kind} and is now ${known.command.kind}`, entry.seq);
    }
    if (known.resolved) throw new Nondeterminism(`command ${entry.seq} was resolved twice`, entry.seq);
    known.resolved = true;
    if (entry.at !== undefined) this.clock = Number(entry.at);
    this.activity += 1;
    if (this.gone(known.command)) return;
    if (entry.cancelled) {
      known.waiting.reject(new EffectError("cancelled", "cancelled", null, entry.seq));
    } else if (entry.error) {
      const { kind, message, data } = entry.error;
      known.waiting.reject(new EffectError(kind || "permanent", message, data, entry.seq));
    } else {
      known.waiting.resolve(entry.ok === undefined ? null : entry.ok);
    }
  }

  cancel(branch, seq) {
    if (typeof seq === "number") this.dropped.add(seq);
    else this.cancelled.add(branch);
    this.activity += 1;
  }

  halt(branch, path) {
    this.activity += 1;
    for (const known of this.commands.values()) {
      const command = known.command;
      if (known.resolved || typeof command.step !== "string") continue;
      const inBranch = command.branch === branch || command.branch.startsWith(`${branch}/`);
      const inPath = command.step === path || command.step.startsWith(`${path}/`);
      if (inBranch && inPath) this.dropped.add(command.seq);
    }
  }

  gone(command) {
    if (this.dropped.has(command.seq)) return true;
    return [...this.cancelled].some((branch) => command.branch === branch || command.branch.startsWith(`${branch}/`));
  }

  attempt(branch, step) {
    const key = `${branch}|${step}`;
    const attempt = (this.attempts.get(key) || 0) + 1;
    this.attempts.set(key, attempt);
    return attempt;
  }

  async drain() {
    let idle = 0;
    let ticks = 0;
    while (idle < IDLE_TICKS && ticks < MOST_TICKS) {
      const before = this.activity;
      await null;
      ticks += 1;
      idle = this.activity === before ? idle + 1 : 0;
    }
  }

  pending() {
    const open = [...this.commands.values()].filter((known) => !known.resolved);
    return {
      pending: open.filter((known) => !this.gone(known.command)).map((known) => known.command),
      cancel: open.filter((known) => this.gone(known.command)).map((known) => known.command.seq),
    };
  }
}

const isCtx = (value) => value instanceof Ctx;

class Ctx {
  constructor(runtime, branch, step, attempt, from) {
    Object.defineProperty(this, "runtime", { value: runtime, enumerable: false });
    this.branch = branch;
    this.step = step === undefined ? null : step;
    this.attempt = attempt === undefined ? 0 : attempt;
    this.from = from === undefined ? null : from;
    this.each = null;
    this.needs = {};
    this.view = null;
  }

  scoped(branch, step, attempt, from) {
    const next = new Ctx(
      this.runtime,
      branch,
      step === undefined ? this.step : step,
      attempt === undefined ? this.attempt : attempt,
      from === undefined ? this.from : from,
    );
    next.each = this.each;
    next.needs = this.needs;
    next.view = this.view;
    return next;
  }

  get item() {
    return this.view || this.runtime.item;
  }

  get workflow() {
    return this.runtime.workflow;
  }

  output(step) {
    const outputs = this.runtime.outputs;
    const base = typeof this.step === "string" ? this.step.split("/") : [];
    const keys = base.map((_, depth) => [...base.slice(0, base.length - 1 - depth), step].join("/"));
    for (const key of keys.length > 0 ? keys : [step]) {
      if (outputs.has(`${this.branch}|${key}`)) return outputs.get(`${this.branch}|${key}`);
      if (outputs.has(key)) return outputs.get(key);
    }
    return null;
  }

  flow(steps, options) {
    const chosen = options || {};
    const path = typeof this.step === "string" ? this.step : "";
    return graph(this, plan(steps, path), { root: false, failFast: chosen.failFast });
  }

  agent(name, input, options) {
    const prompt = typeof input === "string" ? { prompt: input } : input || {};
    const { retry, timeout, connection, ...rest } = options || {};
    const args = { name, input: prompt, ...rest };
    const model = rest.model ?? this.runtime.item.model;
    if (model !== undefined && model !== null) args.model = model;
    const effort = rest.effort ?? this.runtime.item.effort;
    if (effort !== undefined && effort !== null) args.effort = effort;
    return this.runtime.issue(this, "agent", args, { retry, timeout, connection });
  }

  exec(command, options) {
    const { retry, timeout, ...rest } = options || {};
    return this.runtime.issue(this, "exec", { command, ...rest }, { retry, timeout });
  }

  script(name, input, options) {
    const { retry, timeout, ...rest } = options || {};
    const args = { name, input: input === undefined ? {} : input, ...rest };
    return this.runtime.issue(this, "script", args, { retry, timeout });
  }

  call(capability, action, args, options) {
    return this.runtime.issue(this, "call", { capability, action, args: args === undefined ? {} : args }, options);
  }

  waitHuman(view, options) {
    const { assignTo, ...rest } = options || {};
    const args = { view: blocks(view) };
    const assigned = assigneeOf(assignTo);
    if (assigned !== undefined) args.assignTo = assigned;
    return this.runtime.issue(this, "wait.human", args, rest);
  }

  poll(call, every, options) {
    const { retry, timeout, until } = options || {};
    const target = { capability: call.capability, action: call.action, args: call.args === undefined ? {} : call.args };
    const args = { call: target, every, until: until === undefined ? null : until };
    return this.runtime.issue(this, "wait.poll", args, { retry, timeout, connection: call.connection });
  }

  event(kind, filter, options) {
    return this.runtime.issue(this, "event", { kind, filter: filter || {} }, options);
  }

  sleep(ms) {
    return this.runtime.issue(this, "sleep", { ms });
  }

  child(workflow, input, options) {
    return this.runtime.issue(this, "child", { workflow, input: input === undefined ? null : input }, options);
  }

  status(name, options) {
    return this.runtime.issue(this, "status.set", { name }, options);
  }

  link(item) {
    return this.call("items", "link", { item });
  }

  unlink(item) {
    return this.call("items", "unlink", { item });
  }

  accept(on) {
    return this.call("items", "accept", { on: on !== false });
  }

  items(ids) {
    return this.call("items", "get", { items: ids });
  }

  unclaimed(step, state, options) {
    const base = options && options.base ? { base: options.base } : {};
    const lane = options && options.integrated === true ? { integrated: true } : {};
    return this.call("items", "unclaimed", { step, state: state || "passed", ...base, ...lane });
  }

  impediment(on, reason, options) {
    return this.runtime.issue(this, "impediment.set", { on: Boolean(on), reason: reason || null }, options);
  }

  all(branches) {
    return fork(this, "all", branches);
  }

  race(branches) {
    return fork(this, "race", branches);
  }

  any(branches) {
    return fork(this, "any", branches);
  }

  pass(data) {
    return outcome("passed", { data: data === undefined ? null : data });
  }

  fail(data) {
    return outcome("failed", { data: data === undefined ? null : data });
  }

  skip(data) {
    return outcome("skipped", { data: data === undefined ? null : data });
  }

  done(data) {
    return outcome("done", { data: data === undefined ? null : data });
  }

  goto(step, data) {
    return outcome("goto", { to: step, data: data === undefined ? null : data });
  }

  cancel(reason) {
    return outcome("cancelled", { reason: reason || null });
  }

  now() {
    return this.runtime.clock;
  }

  random() {
    return this.runtime.random();
  }
}

function startBranch(parent, id, branch) {
  if (typeof branch === "function") return Promise.resolve().then(() => branch(parent.scoped(id)));
  return Promise.resolve(branch);
}

function settleRace(ctx, ids, list, settled, winner) {
  ids.forEach((id, index) => {
    if (index === winner) return;
    const seq = list[index] && typeof list[index].seq === "number" ? list[index].seq : undefined;
    ctx.runtime.cancel(id, seq);
  });
  ctx.runtime.signal(ctx, "branch.settled", { winner: ids[winner], index: winner, state: settled });
}

function fork(ctx, mode, branches) {
  const list = [...branches];
  const at = ctx.runtime.seq;
  const ids = list.map((_, index) => `${ctx.branch}/${at}.${index}`);
  ctx.runtime.signal(ctx, "fork", { mode, branches: ids });
  const kinds = list.map((branch) => (branch && typeof branch.kind === "string" ? branch.kind : "value"));
  const running = list.map((branch, index) => startBranch(ctx, ids[index], branch));
  const done = deferred();
  const errors = [];
  let left = list.length;
  const results = new Array(list.length);
  let decided = false;
  const decide = (index, state, settle) => {
    if (decided) return;
    decided = true;
    ctx.runtime.activity += 1;
    settleRace(ctx, ids, list, state, index);
    settle();
  };
  running.forEach((promise, index) => {
    promise.then(
      (value) => {
        results[index] = value;
        left -= 1;
        if (mode === "race" || mode === "any") {
          decide(index, "fulfilled", () => done.resolve({ index, branch: ids[index], kind: kinds[index], value }));
        } else if (left === 0 && !decided) {
          decided = true;
          ctx.runtime.activity += 1;
          done.resolve(results);
        }
      },
      (error) => {
        errors[index] = error;
        left -= 1;
        if (mode === "race" || mode === "all") decide(index, "rejected", () => done.reject(error));
        else if (left === 0) decide(index, "rejected", () => done.reject(new AggregateError(errors, "no branch finished successfully")));
      },
    );
  });
  if (list.length === 0) done.resolve(mode === "all" ? [] : null);
  return done.promise;
}

function normalized(value) {
  if (isOutcome(value)) return value;
  return outcome("passed", { data: value === undefined ? null : value });
}

const SETTLED = ["passed", "skipped", "continued"];
const POLICIES = ["ask", "fail", "continue", "skip"];

const objectOf = (value) => (value !== null && typeof value === "object" && !Array.isArray(value) ? value : {});
const callOf = (call) => (typeof call === "function" ? call() : call);
const dataOf = (result) => (result && result.data !== undefined ? result.data : null);

function plain(value) {
  if (value === undefined) return null;
  try {
    return JSON.parse(JSON.stringify(value, (_, inner) => (typeof inner === "function" ? undefined : inner)));
  } catch (error) {
    return null;
  }
}

function trackedBy(item, application) {
  if (item.issue) return {};
  const bound = statusObject(statusObject(statusObject(application).bindings).tracker);
  return { project: bound.connection && bound.value ? bound.value : undefined, connection: bound.connection && bound.value ? bound.connection : undefined };
}

function seenBy(runtime, scope) {
  const key = `${scope.branch}|${scope.step}|${scope.attempt}`;
  if (Object.prototype.hasOwnProperty.call(runtime.seen, key)) return runtime.seen[key];
  return runtime.application;
}

function opened(ctx, call, kind, from) {
  const attempt = ctx.runtime.attempt(ctx.branch, call.id);
  const scope = ctx.scoped(ctx.branch, call.id, attempt, from || null);
  const extra = call.type === "group" ? { group: true } : {};
  const application = seenBy(ctx.runtime, scope);
  const item = ctx.runtime.item;
  if (application && application !== item.application) scope.view = Object.freeze({ ...item, ...trackedBy(item, application), application });
  const seen = application ? { application } : {};
  ctx.runtime.signal(scope, "step.started", { type: kind, options: plain(call.options), ...extra, ...seen });
  return scope;
}

async function guarded(work) {
  try {
    return normalized(await work());
  } catch (error) {
    if (error instanceof Cancelled || error instanceof Nondeterminism) throw error;
    const data = error instanceof EffectError ? { error: error.message, kind: error.kind } : { error: String(error && error.message ? error.message : error) };
    return outcome("failed", { data });
  }
}

function statusObject(value) {
  return value && typeof value === "object" && !Array.isArray(value) ? value : {};
}

const sameStatus = (left, right) => typeof left === "string" && typeof right === "string" && left.toLowerCase() === right.toLowerCase();

export function wantsStatus(value) {
  if (typeof value === "string") return value !== "";
  const wanted = statusObject(value);
  return typeof wanted.category === "string" || Object.keys(statusObject(wanted.byType)).length > 0;
}

export function transitionArgs(value) {
  if (typeof value === "string") return value ? { status: value } : null;
  const wanted = statusObject(value);
  return typeof wanted.category === "string" ? { category: wanted.category } : null;
}

function untracked(item) {
  const found = statusObject(item);
  const application = statusObject(found.application);
  const bound = statusObject(statusObject(application.bindings).tracker);
  return Boolean(application.id) && !found.connection && !(bound.connection && bound.value);
}

async function quietStatus(work) {
  try {
    return statusObject(await work());
  } catch {
    return {};
  }
}

function statusIssue(scope) {
  const item = statusObject(scope.item);
  if (item.issue) return String(item.issue);
  const ticket = statusObject(scope.output("ticket"));
  return ticket.issue ? String(ticket.issue) : null;
}

function statusByType(wanted, type) {
  const named = Object.entries(statusObject(wanted.byType)).find(([name]) => sameStatus(name, type));
  return named && typeof named[1] === "string" ? named[1] : null;
}

function statusIn(statuses, category) {
  const matching = statuses.map(plain).filter((status) => status.category === category && status.name);
  const chosen = category === "done" ? matching[matching.length - 1] : matching[0];
  return chosen ? chosen.name : null;
}

export async function resolveStatus(scope, value) {
  if (typeof value === "string") return value || null;
  const wanted = statusObject(value);
  const options = { optional: true };
  const issue = statusIssue(scope);
  if (!issue && untracked(scope.item)) return null;
  const found = issue ? await quietStatus(() => scope.call("tracker", "issue", { issue }, options)) : {};
  const type = found.type ?? null;
  const fixed = type ? statusByType(wanted, type) : null;
  if (fixed) return fixed;
  if (typeof wanted.category !== "string") return null;
  const project = found.project ?? statusObject(scope.item).project;
  const schema = await quietStatus(() => scope.call("tracker", "schema", project ? { project } : {}, options));
  const types = Array.isArray(schema.types) ? schema.types.map(plain) : [];
  const kind = types.find((entry) => sameStatus(entry.name, type)) ?? types.find((entry) => sameStatus(entry.name, schema.default)) ?? types[0];
  return statusIn(Array.isArray(statusObject(kind).statuses) ? kind.statuses : [], wanted.category);
}

export async function applyStatus(scope, value) {
  if (!wantsStatus(value)) return;
  const name = await resolveStatus(scope, value);
  if (name) await scope.status(name);
}

async function closed(ctx, scope, call, result) {
  const data = dataOf(result);
  if (result.outcome !== "cancelled") {
    ctx.runtime.outputs.set(scope.step, data);
    ctx.runtime.outputs.set(`${ctx.branch}|${scope.step}`, data);
  }
  const finished = result.outcome === "passed" || result.outcome === "skipped";
  if (finished && call.options) await applyStatus(scope, call.options.doneStatus);
  ctx.runtime.signal(scope, "step.finished", { outcome: result.outcome, data, to: result.to || null });
  return result;
}

async function runStep(ctx, type, call, from) {
  const named = { ...call, id: call.id || type.name };
  const scope = opened(ctx, named, type.name, from);
  if (call.options) await applyStatus(scope, call.options.status);
  const result = await guarded(() => type.run(scope, call.options || {}));
  return closed(ctx, scope, named, result);
}

function childrenOf(call) {
  if (call.type === "group") {
    if (Array.isArray(call.steps)) return call.steps;
    const found = registry.workflows.get(call.use);
    if (!found) throw new TypeError(`unknown workflow: ${call.use}`);
    return found.steps;
  }
  const type = registry.steps.get(call.step);
  if (!type) throw new TypeError(`unknown step: ${call.step}`);
  if (!type.steps) return null;
  return typeof type.steps === "function" ? type.steps(call.options || {}) : type.steps;
}

async function runGroup(ctx, node, children, entry, from) {
  const call = { ...node.call, id: node.path };
  const options = call.options || {};
  const scope = opened(ctx, call, node.kind, from);
  await applyStatus(scope, options.status);
  const settings = { root: false, entry, from, failFast: options.failFast };
  const result = await guarded(() => graph(scope, plan(children, node.path), settings));
  return closed(ctx, scope, call, collected(node, result));
}

function collected(node, result) {
  const type = node.call.type === "group" ? null : registry.steps.get(node.call.step);
  if (!type || !type.collect || !["passed", "failed"].includes(result.outcome)) return result;
  const data = objectOf(dataOf(result));
  const steps = result.outcome === "passed" ? data : objectOf(data.steps);
  return outcome(result.outcome, { data: type.collect(steps, result, node.call.options || {}) });
}

function runOnce(ctx, node, entry, from) {
  const children = childrenOf(node.call);
  if (children) return runGroup(ctx, node, children, entry, from);
  return runStep(ctx, registry.steps.get(node.call.step), { ...node.call, id: node.path }, from);
}

function runNode(ctx, node, entry, from) {
  const options = node.call.options || {};
  if (options.forEach === undefined) return runOnce(ctx, node, entry, from);
  return runEach(ctx, node, from);
}

function gathered(results, failed) {
  const data = results.map(dataOf);
  if (!failed) return outcome("passed", { data });
  return outcome("failed", { data: { ...objectOf(dataOf(failed.result)), index: failed.index, results: data } });
}

async function runEach(ctx, node, from) {
  const options = node.call.options || {};
  const given = typeof options.forEach === "function" ? await options.forEach(ctx) : options.forEach;
  const list = Array.isArray(given) ? given : [];
  if (list.length === 0) return outcome("skipped", { data: [] });
  const lanes = Math.max(1, Math.min(list.length, Number(options.concurrency) || list.length));
  const branches = list.length === 1 ? [ctx.branch] : list.map((_, index) => `${ctx.branch}/${node.path}.${index}`);
  const results = new Array(list.length).fill(null);
  const done = deferred();
  const state = { next: 0, left: list.length, over: false, failed: null };
  const stop = (result) => {
    if (state.over) return;
    state.over = true;
    branches.forEach((branch) => ctx.runtime.halt(branch, node.path));
    if (result.outcome !== "cancelled") ctx.runtime.outputs.set(node.path, dataOf(result));
    done.resolve(result);
  };
  const launch = () => {
    if (state.over || state.next >= list.length) return;
    const index = state.next;
    state.next += 1;
    const lane = ctx.scoped(branches[index]);
    lane.each = { value: list[index], index };
    runOnce(lane, node, null, from).then((result) => {
      results[index] = result;
      state.left -= 1;
      if (["cancelled", "done", "goto"].includes(result.outcome)) return stop(result);
      if (result.outcome === "failed") state.failed = state.failed || { index, result };
      if (state.failed && options.failFast !== false) return stop(gathered(results, state.failed));
      if (state.left === 0) return stop(gathered(results, state.failed));
      return launch();
    }, (error) => {
      state.over = true;
      done.reject(error);
    });
  };
  for (let lane = 0; lane < lanes; lane += 1) launch();
  return done.promise;
}

function needsOf(call, index, siblings) {
  const given = call.options ? call.options.needs : undefined;
  if (given === undefined || given === null) return index === 0 ? [] : [siblings[index - 1].id];
  return Array.isArray(given) ? given.map(String) : [String(given)];
}

function checked(nodes, path) {
  const where = path ? ` in ${path}` : "";
  const ids = new Set();
  for (const node of nodes) {
    if (ids.has(node.id)) throw new TypeError(`two steps share the id ${node.id}${where}`);
    ids.add(node.id);
  }
  for (const node of nodes) {
    const unknown = node.needs.find((need) => !ids.has(need));
    if (unknown !== undefined) throw new TypeError(`step ${node.path} needs ${unknown}, which is not beside it`);
  }
  const placed = new Set();
  while (placed.size < nodes.length) {
    const next = nodes.find((node) => !placed.has(node.id) && node.needs.every((need) => placed.has(need)));
    if (!next) throw new TypeError(`the steps${where} need each other in a cycle`);
    placed.add(next.id);
  }
}

function plan(calls, path) {
  const list = (calls || []).map(callOf);
  const nodes = list.map((call, index) => {
    if (!call || typeof call !== "object" || typeof call.id !== "string" || !call.id) throw new TypeError("every step needs an id");
    if (call.id.includes("/")) throw new TypeError(`a step id cannot have a slash: ${call.id}`);
    const kind = call.type === "group" ? "group" : call.step;
    return { id: call.id, call, kind, needs: needsOf(call, index, list), path: path ? `${path}/${call.id}` : call.id };
  });
  checked(nodes, path);
  return nodes;
}

const fresh = () => ({ status: "pending", result: null, from: null, entry: null });

function kept(slot) {
  if (!slot || !SETTLED.includes(slot.status)) return fresh();
  return { ...slot, from: null, entry: null };
}

class Graph {
  constructor(ctx, nodes, options) {
    this.ctx = ctx;
    this.nodes = nodes;
    this.root = Boolean(options.root);
    this.failFast = options.failFast !== false;
    this.byId = new Map(nodes.map((node) => [node.id, node]));
    const key = `${ctx.branch}|${typeof ctx.step === "string" ? ctx.step : ""}`;
    const memory = options.entry ? ctx.runtime.groups.get(key) : null;
    this.state = new Map(nodes.map((node) => [node.id, memory ? kept(memory.get(node.id)) : fresh()]));
    ctx.runtime.groups.set(key, this.state);
    this.running = new Map();
    this.failure = null;
    this.last = null;
  }

  tag(node) {
    return `${this.ctx.branch}|${node.path}`;
  }

  fromOf(node, result) {
    return { step: node.path, outcome: result.outcome, data: dataOf(result) };
  }

  scopeFor(node) {
    const scope = this.ctx.scoped(this.ctx.branch);
    const seen = node.needs.map((id) => {
      const slot = this.state.get(id);
      return [id, { outcome: slot.status, data: dataOf(slot.result) }];
    });
    scope.needs = Object.fromEntries(seen);
    return scope;
  }

  signal(node, kind, data) {
    const attempt = this.ctx.runtime.attempts.get(`${this.ctx.branch}|${node.path}`) || 1;
    this.ctx.runtime.signal(this.ctx.scoped(this.ctx.branch, node.path, attempt, null), kind, data);
  }

  halt(node, state) {
    this.running.delete(node.id);
    this.ctx.runtime.halt(this.ctx.branch, node.path);
    this.signal(node, "step.reset", { state });
  }

  stopAll() {
    for (const id of [...this.running.keys()]) {
      this.halt(this.byId.get(id), "skipped");
      this.state.get(id).status = "cancelled";
    }
  }

  dependents(id) {
    const hit = new Set([id]);
    let grew = true;
    while (grew) {
      grew = false;
      for (const node of this.nodes.filter((node) => !hit.has(node.id))) {
        if (node.needs.some((need) => hit.has(need))) {
          hit.add(node.id);
          grew = true;
        }
      }
    }
    return hit;
  }

  route(target, from, source) {
    const [head, ...rest] = String(target).split("/");
    if (!this.byId.has(head)) return false;
    const hit = this.dependents(head);
    if (source) hit.add(source);
    for (const id of hit) {
      const slot = this.state.get(id);
      const node = this.byId.get(id);
      if (slot.status === "running") this.halt(node, "pending");
      else if (slot.status !== "pending") this.signal(node, "step.reset", { state: "pending" });
      Object.assign(slot, fresh());
    }
    if (this.failure && hit.has(this.failure.node.id)) this.failure = null;
    Object.assign(this.state.get(head), { entry: rest.length > 0 ? rest.join("/") : null, from });
    return true;
  }

  jump(node, to, from, result) {
    if (this.route(to, from, node.id)) return null;
    if (this.root) throw new TypeError(`there is no step ${to} in this workflow`);
    return outcome("goto", { to, from, data: dataOf(result) });
  }

  async decide(node) {
    const states = node.needs.map((id) => this.state.get(id).status);
    if (states.some((status) => status === "pending" || status === "running")) return "wait";
    const options = node.call.options || {};
    const broken = states.some((status) => !SETTLED.includes(status));
    const when = options.when || "success";
    if (when === "success" && (broken || (this.failure && this.failFast))) return "skip";
    if (when === "failure" && !broken && !this.failure) return "skip";
    if (typeof options.if === "function") return (await options.if(this.scopeFor(node))) ? "run" : "skip";
    return options.if === false ? "skip" : "run";
  }

  start(node) {
    const slot = this.state.get(node.id);
    const { entry, from } = slot;
    Object.assign(slot, { status: "running", entry: null, from: null });
    const scope = this.scopeFor(node);
    const id = node.id;
    const promise = Promise.resolve()
      .then(() => runNode(scope, node, entry, from))
      .then((result) => ({ id, result }), (error) => ({ id, error }));
    this.running.set(id, promise);
  }

  async skipped(node) {
    const call = { ...node.call, id: node.path, options: {} };
    const result = outcome("skipped", { data: null });
    Object.assign(this.state.get(node.id), { status: "skipped", result });
    const scope = opened(this.ctx, { ...call, options: node.call.options }, node.kind, null);
    await closed(this.ctx, scope, call, result);
  }

  async schedule() {
    let moved = true;
    while (moved) {
      moved = false;
      for (const node of this.nodes.filter((node) => this.state.get(node.id).status === "pending")) {
        const verdict = await this.decide(node);
        if (verdict === "wait") continue;
        moved = true;
        if (verdict === "run") this.start(node);
        else await this.skipped(node);
      }
    }
  }

  async settle(id, result) {
    const node = this.byId.get(id);
    const slot = this.state.get(id);
    this.last = result;
    if (result.outcome === "passed" || result.outcome === "skipped") {
      Object.assign(slot, { status: result.outcome, result });
      this.ctx.runtime.tries.delete(this.tag(node));
      return null;
    }
    if (result.outcome === "cancelled" || result.outcome === "done") return result;
    if (result.outcome === "goto") return this.jump(node, result.to, result.from || this.fromOf(node, result), result);
    return this.failed(node, result);
  }

  async policy(node, result) {
    const options = node.call.options || {};
    let chosen = options.onFail;
    if (typeof chosen === "function") chosen = await chosen(this.scopeFor(node), result);
    const retries = (given) => Number(given === undefined ? options.retries || 0 : given);
    if (chosen && typeof chosen === "object" && chosen.goto !== undefined) return { goto: String(chosen.goto), retries: retries(chosen.retries) };
    if (typeof chosen === "string" && !POLICIES.includes(chosen)) return { goto: chosen, retries: retries() };
    return { name: typeof chosen === "string" ? chosen : this.root ? "ask" : "fail" };
  }

  async failed(node, result) {
    const runtime = this.ctx.runtime;
    const tag = this.tag(node);
    const slot = this.state.get(node.id);
    const tries = runtime.tries.get(tag) || 0;
    if (tries < Number((node.call.options || {}).rerun || 0)) {
      runtime.tries.set(tag, tries + 1);
      Object.assign(slot, { status: "pending", from: this.fromOf(node, result) });
      return null;
    }
    runtime.tries.delete(tag);
    const chosen = await this.policy(node, result);
    if (chosen.goto === undefined) return this.apply(node, result, chosen.name);
    const used = runtime.gotos.get(tag) || 0;
    if (used >= chosen.retries) return this.apply(node, result, "ask");
    runtime.gotos.set(tag, used + 1);
    return this.jump(node, chosen.goto, this.fromOf(node, result), result);
  }

  async apply(node, result, name) {
    const slot = this.state.get(node.id);
    slot.result = result;
    if (name === "continue") slot.status = "continued";
    else if (name === "skip") this.pass(node);
    else if (name === "ask") return this.ask(node, result);
    else {
      slot.status = "failed";
      if (!this.failure) this.failure = { node, result };
      if (this.failFast) this.stopAll();
    }
    return null;
  }

  pass(node) {
    Object.assign(this.state.get(node.id), { status: "skipped", result: outcome("skipped", { data: null }) });
    this.signal(node, "step.finished", { outcome: "skipped", data: null, to: null });
  }

  async ask(node, result) {
    const decision = await exhausted(this.ctx, node.call, result);
    if (decision.cancel) return decision.cancel;
    this.ctx.runtime.gotos.delete(this.tag(node));
    if (decision.skip) this.pass(node);
    else Object.assign(this.state.get(node.id), { status: "pending", from: this.fromOf(node, result) });
    return null;
  }

  result() {
    if (this.failure) {
      const { node, result } = this.failure;
      return outcome("failed", { data: { ...objectOf(dataOf(result)), failed: node.path, steps: this.outputs() } });
    }
    if (this.root) return this.last;
    return outcome("passed", { data: this.outputs() });
  }

  outputs() {
    return Object.fromEntries(this.nodes.map((node) => [node.id, dataOf(this.state.get(node.id).result)]));
  }
}

async function graph(ctx, nodes, options) {
  const run = new Graph(ctx, nodes, options);
  if (options.entry && !run.route(options.entry, options.from || null, null)) {
    throw new TypeError(`there is no step ${options.entry} in ${ctx.step}`);
  }
  for (;;) {
    await run.schedule();
    if (run.running.size === 0) return run.result();
    const settled = await Promise.race([...run.running.values()]);
    run.running.delete(settled.id);
    if (settled.error) {
      run.stopAll();
      throw settled.error;
    }
    const stop = await run.settle(settled.id, settled.result);
    if (stop) {
      run.stopAll();
      return stop;
    }
  }
}

function nested(ctx, call) {
  const path = typeof ctx.step === "string" && ctx.step ? `${ctx.step}/${call.id}` : call.id;
  return { id: call.id, call, kind: call.step, needs: [], path };
}

export function step(definition) {
  const runs = definition && (typeof definition.run === "function" || definition.steps !== undefined);
  if (!definition || typeof definition.name !== "string" || !runs) {
    throw new TypeError("step({ name, run }) needs name and run or steps");
  }
  const type = Object.freeze({
    name: definition.name,
    view: definition.view || (() => []),
    run: definition.run || null,
    steps: definition.steps === undefined ? null : definition.steps,
    collect: typeof definition.collect === "function" ? definition.collect : null,
    label: definition.label || definition.name,
    icon: definition.icon || null,
    requires: definition.requires || (typeof definition.needs === "string" ? definition.needs : null),
    doing: definition.doing || null,
    delivers: definition.delivers && typeof definition.delivers === "object" ? plain(definition.delivers) : null,
  });
  registry.steps.set(type.name, type);
  const named = typeof definition.id === "string" && definition.id ? definition.id : type.name;
  const use = (first, second) => {
    if (isCtx(first)) {
      const options = second || {};
      const call = { type: "step-call", step: type.name, id: options.id || named, options };
      return runNode(first, nested(first, call), null, null).then((result) => {
        if (result.outcome === "cancelled") throw new Cancelled(result.reason);
        if (result.outcome === "failed") throw new StepFailed(options.id || named, result);
        if (result.outcome === "goto") throw new TypeError("goto only works inside workflow({ steps })");
        return result;
      });
    }
    const options = first || {};
    return Object.freeze({ type: "step-call", step: type.name, id: options.id || named, options });
  };
  Object.defineProperty(use, "stepName", { value: type.name });
  Object.defineProperty(use, "definition", { value: type });
  return use;
}

export function group(definition) {
  if (!definition || typeof definition.id !== "string" || !Array.isArray(definition.steps)) {
    throw new TypeError("group({ id, steps }) needs id and steps");
  }
  const { id, steps, ...options } = definition;
  return Object.freeze({ type: "group", id, steps: steps.map(callOf), options });
}

export function use(target, options) {
  const { id, ...rest } = options || {};
  const name = typeof target === "string" ? target : target && target.name;
  if (typeof name !== "string" || !name) throw new TypeError("use() needs a workflow or its name");
  const steps = target && typeof target === "object" && Array.isArray(target.steps) ? target.steps : null;
  return Object.freeze({ type: "group", id: id || name, use: name, steps, options: rest });
}

const decisions = [
  { id: "retry", label: "Try again", icon: "refresh", variant: "primary" },
  { id: "skip", label: "Skip step", icon: "right" },
  { id: "cancel", label: "Cancel item", icon: "stop", variant: "danger" },
];

function failure(type, call, result) {
  const named = call.options && typeof call.options.label === "string" ? call.options.label : call.id;
  const label = type && type.label ? type.label : named;
  const data = result && result.data && typeof result.data === "object" ? result.data : {};
  const cause = typeof data.error === "string" && data.error.trim() ? data.error.trim() : null;
  return { reason: cause ? `${label} failed: ${cause}` : `${label} failed`, label, cause };
}

async function exhausted(ctx, call, result) {
  const { reason, label, cause } = failure(registry.steps.get(call.step), call, result);
  await ctx.impediment(true, reason);
  const text = cause ? `**${label} failed.** ${cause}` : `**${label} failed.**`;
  const answer = await ctx.waitHuman([markdown(text), actions(decisions)]);
  await ctx.impediment(false);
  const chosen = answer && typeof answer === "object" ? answer.action : answer;
  if (chosen === "cancel") return { cancel: ctx.cancel(reason) };
  return { skip: chosen === "skip", result };
}

async function rooted(ctx, steps) {
  const result = await graph(ctx, plan(steps, ""), { root: true });
  if (result && result.outcome === "failed") throw new StepFailed(objectOf(result.data).failed || "workflow", result);
  return result;
}

export function workflow(definition) {
  if (!definition || typeof definition.name !== "string") throw new TypeError("workflow({ name }) needs name");
  const steps = (definition.steps || []).map(callOf);
  plan(steps, "");
  const run = definition.run || ((ctx) => rooted(ctx, steps));
  const shape = Object.freeze({
    type: "workflow",
    schema_version: definition.schema_version,
    name: definition.name,
    statuses: definition.statuses || [],
    triggers: definition.triggers || [],
    reconcile: definition.reconcile || {},
    accepts: Array.isArray(definition.accepts) ? definition.accepts : [],
    output: definition.output && typeof definition.output === "object" ? plain(definition.output) : delivered(steps, 0) || registered(),
    steps,
    run,
    branches: typeof definition.branches === "function" ? definition.branches : null,
    code: typeof definition.run === "function",
  });
  registry.workflows.set(shape.name, shape);
  return shape;
}

function delivered(calls, depth) {
  if (depth > DEEPEST) return null;
  for (const call of (calls || []).map(callOf)) {
    const type = call && call.type === "step-call" ? registry.steps.get(call.step) : null;
    if (type && type.delivers) return type.delivers;
    const inner = call ? delivered(childrenQuietly(call), depth + 1) : null;
    if (inner) return inner;
  }
  return null;
}

function registered() {
  const found = [...registry.steps.values()].find((type) => type.delivers);
  return found ? found.delivers : null;
}

function childrenQuietly(call) {
  try {
    return childrenOf(call);
  } catch (error) {
    return null;
  }
}

const DEEPEST = 16;

function tree(calls, path, nodes, depth) {
  if (depth > DEEPEST) return [];
  const list = (calls || []).map(callOf);
  return list.map((call, index) => {
    const at = path ? `${path}/${call.id}` : call.id;
    const options = objectOf(call.options);
    const shaped = { id: call.id, path: at, step: call.type === "group" ? "group" : call.step, needs: needsOf(call, index, list), options: plain(options) };
    if (call.type === "group" || options.label || options.icon || options.doing) {
      const label = options.label || (call.type === "group" ? call.id : null);
      nodes[at] = { label, icon: options.icon || null, doing: options.doing || null };
    }
    const children = childrenQuietly(call);
    if (children) shaped.steps = tree(children, at, nodes, depth + 1);
    return shaped;
  });
}

function statusesOf(calls, named, depth) {
  if (depth > DEEPEST) return named;
  for (const call of (calls || []).map(callOf)) {
    const options = call && call.options ? call.options : {};
    for (const name of [options.status, options.doneStatus]) {
      if (typeof name === "string" && name && !named.includes(name)) named.push(name);
    }
    const children = call ? childrenQuietly(call) : null;
    if (children) statusesOf(children, named, depth + 1);
  }
  return named;
}

function declared(found) {
  const named = statusesOf(found.steps, [...found.statuses], 0);
  return Object.freeze({ name: found.name, statuses: Object.freeze(named) });
}

function target(activation, runtime) {
  const wanted = activation.target || {};
  if (wanted.step !== undefined) {
    const type = registry.steps.get(wanted.step);
    if (!type) throw new TypeError(`unknown step: ${wanted.step}`);
    return (ctx) => runStep(ctx, type, { id: wanted.id || type.name, options: wanted.options || {} });
  }
  const name = wanted.workflow === undefined ? [...registry.workflows.keys()][0] : wanted.workflow;
  const found = registry.workflows.get(name);
  if (!found) throw new TypeError(`unknown workflow: ${name}`);
  runtime.workflow = declared(found);
  return (ctx, item) => found.run(ctx, item);
}

function ended(runtime, value) {
  runtime.activity += 1;
  if (isOutcome(value) && value.outcome === "cancelled") {
    runtime.finished = { state: "cancelled", reason: value.reason };
  } else if (isOutcome(value) && value.outcome === "done") {
    runtime.finished = { state: "completed", result: value.data === undefined ? null : value.data };
  } else {
    runtime.finished = { state: "completed", result: value === undefined ? null : value };
  }
}

function failed(runtime, error) {
  runtime.activity += 1;
  if (error instanceof Cancelled) {
    runtime.finished = { state: "cancelled", reason: error.reason };
  } else if (error instanceof Nondeterminism) {
    runtime.finished = { state: "nondeterministic", error: { kind: "nondeterminism", message: error.message, seq: error.seq } };
  } else {
    const kind = error instanceof EffectError ? error.kind : "error";
    const message = error && error.message ? error.message : String(error);
    runtime.finished = { state: "failed", error: { kind, message, data: error && error.data !== undefined ? error.data : null } };
  }
}

async function activate(activation) {
  const input = activation || {};
  const runtime = new Runtime(input);
  try {
    const handler = target(input, runtime);
    const ctx = new Ctx(runtime, ROOT);
    Promise.resolve()
      .then(() => handler(ctx, input.item || {}))
      .then((value) => ended(runtime, value), (error) => failed(runtime, error));
    await runtime.drain();
    for (const entry of input.journal || []) {
      if (runtime.finished) break;
      runtime.deliver(entry);
      await runtime.drain();
    }
  } catch (error) {
    failed(runtime, error);
  }
  const finished = runtime.finished || { state: "running" };
  const open = runtime.pending();
  return {
    ...finished,
    pending: finished.state === "running" ? open.pending : [],
    cancel: finished.state === "running" ? open.cancel : open.pending.map((command) => command.seq).concat(open.cancel),
    signals: runtime.signals,
    seq: runtime.seq,
  };
}

function view(request) {
  const type = registry.steps.get(request.step);
  if (!type) throw new TypeError(`unknown step: ${request.step}`);
  return blocks(type.view(request.state || {}));
}

function describe() {
  const labels = Object.fromEntries(
    [...registry.steps.values()].map((type) => [type.name, { label: type.label, icon: type.icon, requires: type.requires, doing: type.doing }]),
  );
  const nodes = {};
  const workflows = [...registry.workflows.values()].map((found) => ({
    schema_version: found.schema_version,
    name: found.name,
    statuses: found.statuses,
    triggers: found.triggers,
    reconcile: found.reconcile,
    accepts: found.accepts,
    output: found.output,
    steps: tree(found.steps, "", nodes, 0),
    code: found.code,
  }));
  return { workflows, steps: [...registry.steps.keys()], labels, nodes };
}

const BRANCH_FIELDS = ["head", "base", "method"];

function branch(target, asked) {
  const found = registry.workflows.get(objectOf(target).workflow);
  if (!found || !found.branches) return null;
  const given = objectOf(found.branches(objectOf(asked)));
  const chosen = {};
  for (const name of BRANCH_FIELDS) {
    if (typeof given[name] === "string" && given[name].trim() !== "") chosen[name] = given[name].trim();
  }
  if (Array.isArray(given.promote)) chosen.promote = given.promote.map(objectOf).filter((step) => step.from && step.into);
  return Object.keys(chosen).length > 0 ? chosen : null;
}

export const PROTOCOL = 1;

export const host = Object.freeze({ protocol: PROTOCOL, activate, view, describe, branch });
