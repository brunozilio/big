import { type Creds, type Language, connect, fail, optionalText, pick, text, tokenField, unknownAction } from "sdk/connectors/index.ts";
import { binding, bindingField } from "sdk/connectors/contracts.ts";
import { purchasing } from "sdk/connectors/purchasing.ts";
import { INVOICES, ORDERS, priced, totalOf } from "./books.ts";

export const KEY = "mock-purchasing";
const MEMORY = `${KEY}:ledger`;
const DAY = 86400;
const TERM_DAYS = 28;

type Ledger = { matched: Record<string, string>; released: Record<string, string> };

const emptyLedger = (): Ledger => ({ matched: {}, released: {} });

function invoice(args: Record<string, unknown>) {
  const number = text(args, "invoice");
  const found = INVOICES[number] ?? fail("permanent", `the invoice ${number} was not received`);
  const status = (ledger: Ledger) => (ledger.released[number] ? "released" : ledger.matched[number] ? "matched" : "received");
  return { found, status };
}

function order(args: Record<string, unknown>) {
  const number = text(args, "order");
  const found = ORDERS[number] ?? fail("permanent", `the purchase order ${number} does not exist`);
  return { ...found, lines: priced(found.lines), total: totalOf(found.lines) };
}

function match(ledger: Ledger, args: Record<string, unknown>) {
  const { found } = invoice(args);
  const wanted = text(args, "order");
  if (found.order !== wanted) fail("conflict", `the invoice ${found.number} cites the order ${found.order}, not ${wanted}`);
  ledger.matched[found.number] = wanted;
  return { invoice: found.number, order: wanted, status: "matched", ref: `CONF-${found.number}` };
}

function release(ledger: Ledger, args: Record<string, unknown>, now: number) {
  const { found } = invoice(args);
  if (!ledger.matched[found.number]) fail("conflict", `the invoice ${found.number} was not matched to its order yet`);
  const due = optionalText(args, "due") ?? new Date((now + TERM_DAYS * DAY) * 1000).toISOString().slice(0, 10);
  ledger.released[found.number] = due;
  return { invoice: found.number, order: found.order, status: "released", due, ref: `PAG-${found.number}` };
}

export function run({ action, args, ledger, now }: { action: string; args: Record<string, unknown>; ledger: Ledger | null; now: number }) {
  const held: Ledger = ledger ?? emptyLedger();
  const value = (() => {
    if (action === "purchasing.invoice") {
      const { found, status } = invoice(args);
      return { ...found, lines: priced(found.lines), total: totalOf(found.lines), status: status(held) };
    }
    if (action === "purchasing.order") return order(args);
    if (action === "purchasing.match") return match(held, args);
    if (action === "purchasing.release") return release(held, args, now);
    return unknownAction(action);
  })();
  return { value, ledger: held };
}

export function provider() {
  const name = tokenField("name", "Name", "Any name: this simulated purchasing system keeps its records in memory.", false);
  return {
    schema_version: 2,
    key: KEY,
    name: "Purchasing (mock)",
    logo: "logo.svg",
    capabilities: ["purchasing"],
    auth: { kind: "token", fields: [name], form: "bearer" },
    actions: purchasing(),
  };
}

export function presentation({ language }: { language: Language }) {
  const company = bindingField("company", pick(language, "Company", "Empresa", "Empresa"));
  return {
    description: pick(language, "Simulated invoices and purchase orders", "Facturas y órdenes de compra simuladas", "Notas fiscais e pedidos de compra simulados"),
    group: "purchasing",
    button: connect(language),
    site: pick(language, "the simulated system", "del sistema simulado", "do sistema simulado"),
    choose_title: "",
    hidden: false,
    bindings: [binding("purchasing", "application", pick(language, "Purchasing", "Compras", "Compras"), { fields: [company], order: 85 })],
  };
}

export function call({ action, args }: { action: string; args: Record<string, unknown>; creds: Creds }, ctx) {
  const ran = run({ action, args, ledger: ctx.memory.get({ name: MEMORY }), now: ctx.clock.now() });
  ctx.memory.set({ name: MEMORY, value: ran.ledger });
  return ran.value;
}

export function accounts({ creds }: { creds: Creds }) {
  const label = creds.token.trim() === "" ? KEY : creds.token;
  return [{ id: KEY, label, settings: {} }];
}
