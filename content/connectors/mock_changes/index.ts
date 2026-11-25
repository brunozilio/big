import { type Creds, type Language, connect, fail, pick, text, tokenField, unknownAction } from "sdk/connectors/index.ts";
import { changes } from "sdk/connectors/contracts.ts";

export const KEY = "mock-changes";
export const AWAITING = "awaiting_approval";
export const EXECUTING = "executing";
export const DONE = "done";
export const CANCELLED = "cancelled";
const KEY_BASE = 4830;
const UPDATABLE = ["title", "window", "tasks", "notes", "attachments", "versions", "description"];
const MEMORY = `${KEY}:ledger`;

type Pace = { approval: number; execution: number };
type Change = { created: number; seq: number; revision: number; cancelled: boolean; data: Record<string, unknown> };
type Ledger = { seq: number; keys: Record<string, string>; changes: Record<string, Change> };

const emptyLedger = (): Ledger => ({ seq: 0, keys: {}, changes: {} });

export function status(pace: Pace, created: number, now: number): string {
  const elapsed = now - created;
  if (elapsed < pace.approval) return AWAITING;
  if (elapsed < pace.approval + pace.execution) return EXECUTING;
  return DONE;
}

const turns = (pace: Pace, created: number): [number, string][] => [
  [created + pace.approval, EXECUTING],
  [created + pace.approval + pace.execution, DONE],
];

function parseRef(reference: string): [number, number] | undefined {
  const match = /^mock-(-?\d+)-(\d+)$/.exec(reference);
  return match ? [Number(match[1]), Number(match[2])] : undefined;
}

function entry(ledger: Ledger, reference: string): Change {
  if (!(reference in ledger.changes)) {
    const parsed = parseRef(reference) ?? fail("permanent", `the change ${reference} does not exist`);
    ledger.changes[reference] = { created: parsed[0], seq: parsed[1], revision: 0, cancelled: false, data: {} };
  }
  return ledger.changes[reference];
}

function shown(reference: string, change: Change, pace: Pace, now: number) {
  const out: Record<string, unknown> = {
    ref: reference,
    key: `GMUD-${KEY_BASE + change.seq}`,
    status: change.cancelled ? CANCELLED : status(pace, change.created, now),
    revision: change.revision,
  };
  for (const name of ["title", "environment", "window"]) {
    if (name in change.data) out[name] = change.data[name];
  }
  return out;
}

function create(ledger: Ledger, args: Record<string, unknown>, key: string, pace: Pace, now: number) {
  for (const name of ["title", "environment", "window"]) text(args, name);
  const known = key !== "" ? ledger.keys[key] : undefined;
  if (known !== undefined) return { ...shown(known, entry(ledger, known), pace, now), created: false };
  ledger.seq += 1;
  const reference = `mock-${now}-${ledger.seq}`;
  const change = { created: now, seq: ledger.seq, revision: 0, cancelled: false, data: { ...args } };
  ledger.changes[reference] = change;
  if (key !== "") ledger.keys[key] = reference;
  return { ...shown(reference, change, pace, now), created: true };
}

function update(ledger: Ledger, args: Record<string, unknown>, pace: Pace, now: number) {
  const reference = text(args, "change");
  const change = entry(ledger, reference);
  if (change.cancelled) fail("permanent", `the change ${reference} was cancelled`);
  for (const name of UPDATABLE) {
    if (args[name] !== undefined && args[name] !== null) change.data[name] = args[name];
  }
  change.revision += 1;
  return shown(reference, change, pace, now);
}

function read(ledger: Ledger, args: Record<string, unknown>, pace: Pace, now: number, cancel: boolean) {
  const reference = text(args, "change");
  const change = entry(ledger, reference);
  if (cancel) change.cancelled = true;
  return shown(reference, change, pace, now);
}

export function run({ action, args, key, ledger, pace, now }) {
  const held: Ledger = ledger ?? emptyLedger();
  const value = (() => {
    if (action === "changes.create") return create(held, args, key ?? "", pace, now);
    if (action === "changes.update") return update(held, args, pace, now);
    if (action === "changes.get") return read(held, args, pace, now, false);
    if (action === "changes.cancel") return read(held, args, pace, now, true);
    return unknownAction(action);
  })();
  return { value, ledger: held };
}

export function eventsBetween({ ledger, pace, since, now }) {
  const found: { id: string; capability: string; kind: string; subject: string; data: unknown; at: string }[] = [];
  for (const [reference, change] of Object.entries((ledger ?? emptyLedger()).changes as Record<string, Change>)) {
    if (change.cancelled) continue;
    for (const [at, turned] of turns(pace, change.created)) {
      if (at > since && at <= now) {
        found.push({
          id: `${reference}:${turned}`,
          capability: "changes",
          kind: "changes.status.changed",
          subject: reference,
          data: { ref: reference, status: turned },
          at: new Date(at * 1000).toISOString(),
        });
      }
    }
  }
  return found.sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
}

function paceOf(ctx): Pace {
  const seconds = (value: unknown) => {
    const parsed = Number.parseInt(String(value ?? "").trim(), 10);
    return Number.isFinite(parsed) ? parsed : 30;
  };
  return { approval: seconds(ctx.config?.approval_seconds), execution: seconds(ctx.config?.execution_seconds) };
}

const nameField = (language: Language) =>
  tokenField(
    "name",
    pick(language, "Name", "Nombre", "Nome"),
    pick(
      language,
      "Any name: this simulated system keeps the changes in memory.",
      "Cualquier nombre: este sistema simulado guarda los cambios en memoria.",
      "Qualquer nome: este sistema simulado guarda as mudanças na memória.",
    ),
    false,
  );

export function provider() {
  return {
    key: KEY,
    name: "Change requests (mock)",
    logo: "logo.svg",
    capabilities: ["changes"],
    auth: { kind: "token", fields: [nameField("pt-BR")], form: "bearer" },
    actions: changes(),
  };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(language, "Simulated change requests", "Cambios simulados", "Mudanças simuladas"),
    group: "changes",
    button: connect(language),
    site: pick(language, "the simulated system", "del sistema simulado", "do sistema simulado"),
    choose_title: "",
    hidden: false,
  };
}

export function call({ action, args, idempotencyKey }, ctx) {
  const ran = run({ action, args, key: idempotencyKey, ledger: ctx.memory.get({ name: MEMORY }), pace: paceOf(ctx), now: ctx.clock.now() });
  ctx.memory.set({ name: MEMORY, value: ran.ledger });
  return ran.value;
}

export function accounts({ creds }: { creds: Creds }) {
  const label = creds.token.trim() === "" ? KEY : creds.token;
  return [{ id: KEY, label, settings: {} }];
}

export function poll({ cursor }: { cursor: string | null }, ctx) {
  const now = ctx.clock.now();
  const since = cursor !== null && cursor !== undefined && /^-?\d+$/.test(cursor) ? Number(cursor) : undefined;
  const events = since === undefined ? [] : eventsBetween({ ledger: ctx.memory.get({ name: MEMORY }), pace: paceOf(ctx), since, now });
  return { events, cursor: String(now) };
}
