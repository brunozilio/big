import { object } from "./context.js";

export async function given(ctx, value) {
  return typeof value === "function" ? value(ctx.item, ctx) : value;
}

export function draftOf(ctx, opts, fallback) {
  const found = object(ctx.output(opts.of ?? fallback));
  const text = found.draft ?? found.text ?? found.summary;
  if (typeof text === "string") return { found, text };
  return { found, text: Object.keys(found).length > 0 ? JSON.stringify(found, null, 2) : "" };
}

const REJECTED = /(?:VERDICT|VEREDITO|VEREDICTO):\s*(?:REJECTED|REPROVADO|RECHAZADO)/i;

export function judged(result) {
  const output = object(result.output);
  const text = String(result.text ?? "");
  const said = typeof output.verdict === "string" ? output.verdict : "";
  const approved = typeof output.approved === "boolean" ? output.approved : said ? said === "approve" : !REJECTED.test(text);
  const reason = output.reason ?? output.summary ?? output.body ?? text;
  return { approved, reason: String(reason ?? ""), session: result.session };
}

export function fieldsText(fields) {
  const entries = Object.entries(object(fields)).filter(([, value]) => value !== undefined && value !== null && value !== "");
  return entries.map(([key, value]) => `- ${key}: ${typeof value === "string" ? value : JSON.stringify(value)}`).join("\n");
}
