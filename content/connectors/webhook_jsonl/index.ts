import { type ConfigSchema, type Creds, type Language, connect, fail, fetch, optionalField, pick, setting, settingField, tokenField, unknownAction } from "sdk/connectors/index.ts";
import { type AuditAck, audit } from "sdk/connectors/contracts.ts";

export const KEY = "webhook_jsonl";
const URL_SETTING = "url";

export const config: ConfigSchema = {
  url: { description: "Where the events go when the connection names no address" },
  batch: { type: "integer", default: 100, description: "The most events one request carries" },
};

type Event = Record<string, unknown> & { id?: string };

export function lines(events: Event[]): string {
  return events.map((event) => JSON.stringify(event)).join("\n") + "\n";
}

export function batchOf(value: unknown): number {
  const parsed = Math.floor(Number(value));
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 100;
}

export function headersOf(sent: Event[], token: string): Record<string, string> {
  const key = `${sent[0]?.id ?? ""}..${sent[sent.length - 1]?.id ?? ""}`;
  const headers: Record<string, string> = { "content-type": "application/x-ndjson", "idempotency-key": key };
  if (token.trim() !== "") headers.authorization = `Bearer ${token.trim()}`;
  return headers;
}

export function deliver(ctx, creds: Creds, events: Event[]): AuditAck {
  if (events.length === 0) return { ack: 0 };
  const url = (setting(creds, URL_SETTING) ?? String(ctx.config?.url ?? "")).trim();
  if (url === "") fail("permanent", "the sink names no destination: set url in the connection or in [webhook_jsonl] of config.toml");
  const sent = events.slice(0, batchOf(ctx.config?.batch));
  fetch(ctx, { method: "POST", url, headers: headersOf(sent, creds?.token ?? ""), body: lines(sent) });
  return { ack: sent.length };
}

function fieldsOf(language: Language) {
  const address = pick(language, "Address", "Dirección", "Endereço");
  const where = pick(language, "Where the audit events are posted.", "Adónde se envían los eventos de auditoría.", "Para onde os eventos de auditoria são enviados.");
  const bearer = pick(language, "Sent as a bearer token, when given.", "Se envía como bearer token, si se da.", "Vai como bearer token, se informado.");
  return [settingField(URL_SETTING, address, where), optionalField(tokenField("token", "Token", bearer, true))];
}

export function provider() {
  return {
    schema_version: 2,
    key: KEY,
    name: "Webhook (JSON Lines)",
    logo: "logo.svg",
    capabilities: ["audit"],
    auth: { kind: "token", fields: fieldsOf("en"), form: "bearer" },
    actions: audit(),
  };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(language, "Audit events posted as JSON Lines", "Eventos de auditoría enviados como JSON Lines", "Eventos de auditoria enviados em JSON Lines"),
    group: "audit",
    button: connect(language),
    site: pick(language, "the webhook", "del webhook", "do webhook"),
    choose_title: "",
    hidden: false,
  };
}

export function call({ action, args, creds }: { action: string; args: Record<string, unknown>; creds: Creds }, ctx) {
  if (action === "audit.deliver") return deliver(ctx, creds, Array.isArray(args?.events) ? (args.events as Event[]) : []);
  return unknownAction(action);
}

export function accounts({ creds }: { creds: Creds }) {
  return [{ id: KEY, label: setting(creds, URL_SETTING) ?? KEY, settings: {} }];
}
