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
    super(`a etapa ${step} falhou`);
    this.name = "StepFailed";
    this.step = step;
    this.outcome = outcome;
  }
}

export class Cancelled extends Error {
  constructor(reason) {
    super(reason || "cancelado");
    this.name = "Cancelled";
    this.reason = reason || null;
  }
}

class Nondeterminism extends Error {
  constructor(message) {
    super(message);
    this.name = "Nondeterminism";
  }
}

const block = (type, fields) => Object.freeze({ type, ...fields });

export const chat = (session, options) => block("chat", { session, composer: Boolean(options && options.composer) });
export const issue = (ref) => block("issue", { ref });
export const change = (pr) => block("change", { pr });
export const review = (pr) => block("review", { pr });
export const pipeline = (run) => block("pipeline", { run });
export const merge = (pr, methods) => block("merge", { pr, methods: methods || [] });
export const markdown = (text) => block("markdown", { text: String(text === undefined ? "" : text) });
export const logs = (text) => block("logs", { text: String(text === undefined ? "" : text) });
export const form = (fields) => block("form", { fields: fields || [] });
export const actions = (buttons) =>
  block("actions", {
    buttons: (buttons || []).map((button) => (typeof button === "string" ? { id: button, label: button } : button)),
  });

const blocks = (view) => (view === undefined || view === null ? [] : Array.isArray(view) ? view : [view]);

export const on = (kind, filter) => Object.freeze({ type: "trigger", kind, filter: filter || {} });

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
    this.item = Object.freeze({ ...(activation.item || {}) });
    this.clock = Number(activation.start || 0);
    this.random = generator(String(activation.seed === undefined ? "" : activation.seed));
    this.finished = null;
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
    for (const key of ["connection", "retry", "timeout"]) {
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
    if (!known) throw new Nondeterminism(`o comando ${entry.seq} não foi emitido nesta reexecução`);
    if (entry.kind !== undefined && entry.kind !== known.command.kind) {
      throw new Nondeterminism(`o comando ${entry.seq} era ${entry.kind} e agora é ${known.command.kind}`);
    }
    if (known.resolved) throw new Nondeterminism(`o comando ${entry.seq} foi resolvido duas vezes`);
    known.resolved = true;
    if (entry.at !== undefined) this.clock = Number(entry.at);
    this.activity += 1;
    if (this.gone(known.command)) return;
    if (entry.cancelled) {
      known.waiting.reject(new EffectError("cancelled", "cancelado", null, entry.seq));
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
  }

  scoped(branch, step, attempt, from) {
    return new Ctx(
      this.runtime,
      branch,
      step === undefined ? this.step : step,
      attempt === undefined ? this.attempt : attempt,
      from === undefined ? this.from : from,
    );
  }

  get item() {
    return this.runtime.item;
  }

  output(step) {
    const found = this.runtime.outputs.get(step);
    return found === undefined ? null : found;
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

  call(capability, action, args, options) {
    return this.runtime.issue(this, "call", { capability, action, args: args === undefined ? {} : args }, options);
  }

  waitHuman(view, options) {
    return this.runtime.issue(this, "wait.human", { view: blocks(view) }, options);
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
        else if (left === 0) decide(index, "rejected", () => done.reject(new AggregateError(errors, "nenhum ramo terminou bem")));
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

async function runStep(ctx, type, call, from) {
  const id = call.id || type.name;
  const attempt = ctx.runtime.attempt(ctx.branch, id);
  const scope = ctx.scoped(ctx.branch, id, attempt, from || null);
  ctx.runtime.signal(scope, "step.started", { type: type.name, options: call.options });
  if (call.options && typeof call.options.status === "string") await scope.status(call.options.status);
  let result;
  try {
    result = normalized(await type.run(scope, call.options || {}));
  } catch (error) {
    if (error instanceof Cancelled || error instanceof Nondeterminism) throw error;
    const data = error instanceof EffectError ? { error: error.message, kind: error.kind } : { error: String(error && error.message ? error.message : error) };
    result = outcome("failed", { data });
  }
  if (result.outcome !== "cancelled") ctx.runtime.outputs.set(id, result.data === undefined ? null : result.data);
  if (result.outcome === "passed" && call.options && typeof call.options.doneStatus === "string") {
    await scope.status(call.options.doneStatus);
  }
  ctx.runtime.signal(scope, "step.finished", { outcome: result.outcome, data: result.data === undefined ? null : result.data, to: result.to || null });
  return result;
}

export function step(definition) {
  if (!definition || typeof definition.name !== "string" || typeof definition.run !== "function") {
    throw new TypeError("step({ name, run }) precisa de name e run");
  }
  const type = Object.freeze({
    name: definition.name,
    view: definition.view || (() => []),
    run: definition.run,
    label: definition.label || definition.name,
    icon: definition.icon || null,
    needs: definition.needs || null,
    doing: definition.doing || null,
  });
  registry.steps.set(type.name, type);
  const use = (first, second) => {
    if (isCtx(first)) {
      const options = second || {};
      return runStep(first, type, { id: options.id, options }).then((result) => {
        if (result.outcome === "cancelled") throw new Cancelled(result.reason);
        if (result.outcome === "failed") throw new StepFailed(options.id || type.name, result);
        if (result.outcome === "goto") throw new TypeError("goto só vale dentro de workflow({ steps })");
        return result;
      });
    }
    const options = first || {};
    return Object.freeze({ type: "step-call", step: type.name, id: options.id || type.name, options });
  };
  Object.defineProperty(use, "stepName", { value: type.name });
  Object.defineProperty(use, "definition", { value: type });
  return use;
}

function indexOf(steps, id) {
  const index = steps.findIndex((call) => call.id === id);
  if (index < 0) throw new TypeError(`não existe etapa ${id} neste workflow`);
  return index;
}

const decisions = [
  { id: "retry", label: "Tentar de novo", icon: "refresh", variant: "primary" },
  { id: "skip", label: "Pular etapa", icon: "right" },
  { id: "cancel", label: "Cancelar item", icon: "stop", variant: "danger" },
];

function failure(type, call, result) {
  const label = type && type.label ? type.label : call.id;
  const data = result && result.data && typeof result.data === "object" ? result.data : {};
  const cause = typeof data.error === "string" && data.error.trim() ? data.error.trim() : null;
  return { reason: cause ? `${label} falhou: ${cause}` : `${label} falhou`, label, cause };
}

async function exhausted(ctx, call, result) {
  const { reason, label, cause } = failure(registry.steps.get(call.step), call, result);
  await ctx.impediment(true, reason);
  const text = cause ? `**${label} falhou.** ${cause}` : `**${label} falhou.**`;
  const answer = await ctx.waitHuman([markdown(text), actions(decisions)]);
  await ctx.impediment(false);
  const chosen = answer && typeof answer === "object" ? answer.action : answer;
  if (chosen === "cancel") return { cancel: ctx.cancel(reason) };
  return { skip: chosen === "skip", result };
}

async function sequence(ctx, steps) {
  const retries = new Map();
  let at = 0;
  let last = null;
  let from = null;
  while (at < steps.length) {
    const call = steps[at];
    const type = registry.steps.get(call.step);
    if (!type) throw new TypeError(`etapa desconhecida: ${call.step}`);
    const result = await runStep(ctx, type, call, from);
    last = result;
    const moved = result.outcome === "passed" || result.outcome === "skipped";
    from = moved ? null : { step: call.id, outcome: result.outcome, data: result.data === undefined ? null : result.data };
    if (result.outcome === "cancelled") return result;
    if (moved) at += 1;
    else if (result.outcome === "goto") at = indexOf(steps, result.to);
    else {
      const used = retries.get(call.id) || 0;
      const allowed = call.options.retries === undefined ? 0 : call.options.retries;
      if (call.options.onFail && used < allowed) {
        retries.set(call.id, used + 1);
        at = indexOf(steps, call.options.onFail);
        continue;
      }
      const decision = await exhausted(ctx, call, result);
      if (decision.cancel) return decision.cancel;
      retries.set(call.id, 0);
      if (decision.skip) {
        at += 1;
        from = null;
      }
    }
  }
  return last;
}

export function workflow(definition) {
  if (!definition || typeof definition.name !== "string") throw new TypeError("workflow({ name }) precisa de name");
  const steps = (definition.steps || []).map((call) => (typeof call === "function" ? call() : call));
  const run = definition.run || ((ctx) => sequence(ctx, steps));
  const shape = Object.freeze({
    type: "workflow",
    name: definition.name,
    statuses: definition.statuses || [],
    triggers: definition.triggers || [],
    reconcile: definition.reconcile || {},
    steps,
    run,
    code: typeof definition.run === "function",
  });
  registry.workflows.set(shape.name, shape);
  return shape;
}

function target(activation) {
  const wanted = activation.target || {};
  if (wanted.step !== undefined) {
    const type = registry.steps.get(wanted.step);
    if (!type) throw new TypeError(`etapa desconhecida: ${wanted.step}`);
    return (ctx) => runStep(ctx, type, { id: wanted.id || type.name, options: wanted.options || {} });
  }
  const name = wanted.workflow === undefined ? [...registry.workflows.keys()][0] : wanted.workflow;
  const found = registry.workflows.get(name);
  if (!found) throw new TypeError(`workflow desconhecido: ${name}`);
  return (ctx, item) => found.run(ctx, item);
}

function ended(runtime, value) {
  runtime.activity += 1;
  if (isOutcome(value) && value.outcome === "cancelled") {
    runtime.finished = { state: "cancelled", reason: value.reason };
  } else {
    runtime.finished = { state: "completed", result: value === undefined ? null : value };
  }
}

function failed(runtime, error) {
  runtime.activity += 1;
  if (error instanceof Cancelled) {
    runtime.finished = { state: "cancelled", reason: error.reason };
  } else if (error instanceof Nondeterminism) {
    runtime.finished = { state: "nondeterministic", error: { kind: "nondeterminism", message: error.message } };
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
    const handler = target(input);
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
  if (!type) throw new TypeError(`etapa desconhecida: ${request.step}`);
  return blocks(type.view(request.state || {}));
}

function describe() {
  return {
    workflows: [...registry.workflows.values()].map((found) => ({
      name: found.name,
      statuses: found.statuses,
      triggers: found.triggers,
      reconcile: found.reconcile,
      steps: found.steps,
      code: found.code,
    })),
    steps: [...registry.steps.keys()],
    labels: Object.fromEntries(
      [...registry.steps.values()].map((type) => [type.name, { label: type.label, icon: type.icon, needs: type.needs, doing: type.doing }]),
    ),
  };
}

export const PROTOCOL = 1;

export const host = Object.freeze({ protocol: PROTOCOL, activate, view, describe });
