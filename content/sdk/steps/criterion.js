import { step, chat, logs } from "sdk";
import { agentOptions, defined, inLanguage, object } from "./context.js";
import { fieldsText, given, judged } from "./given.js";

function listed(criterion) {
  const all = typeof criterion === "function" ? [criterion] : Array.isArray(criterion) ? criterion : [];
  return all.map((entry, index) => {
    const fallback = `criterion ${index + 1}`;
    if (typeof entry === "function") return { name: entry.name || fallback, check: entry };
    const found = object(entry);
    return { name: String(found.name ?? fallback), check: found.check };
  });
}

function verdictOf(name, value) {
  if (typeof value === "boolean") return { name, met: value };
  const found = object(value);
  const met = found.met ?? found.ok ?? found.passed;
  return defined({ name, met: met === true, reason: found.reason === undefined ? undefined : String(found.reason) });
}

async function checked(ctx, opts) {
  const results = [];
  for (const entry of listed(opts.criterion)) {
    if (typeof entry.check !== "function") {
      results.push({ name: entry.name, met: false, reason: "the criterion is not a function" });
      continue;
    }
    results.push(verdictOf(entry.name, await entry.check(ctx.item, ctx)));
  }
  return results;
}

const lineOf = (result) => `${result.met ? "✓" : "✗"} ${result.name}${result.reason ? `: ${result.reason}` : ""}`;

async function byAgent(ctx, opts, results) {
  const written = await given(ctx, opts.prompt);
  const parts = [written ? String(written) : `Verify the item: ${ctx.item.title ?? ""}`];
  const fields = fieldsText(await given(ctx, opts.fields));
  if (fields) parts.push(`## Fields\n\n${fields}`);
  if (results.length > 0) parts.push(`## Criteria already met\n\n${results.map(lineOf).join("\n")}`);
  parts.push("End with exactly `VERDICT: APPROVED` or `VERDICT: REJECTED`.");
  const result = await ctx.agent(opts.agent, { prompt: inLanguage(ctx, parts.join("\n\n")) }, agentOptions(opts, { mode: opts.mode ?? "plan", inPlace: true }));
  return judged(result);
}

function shown(s) {
  const results = Array.isArray(s.results) ? s.results : [];
  const lines = results.map((result) => ({ name: result.name, state: result.met ? "done" : "failed", seconds: null, log: result.reason ?? "" }));
  if (s.session) lines.push({ name: "Verifier", state: s.approved === false ? "failed" : "done", seconds: null, log: s.verdict ?? "", session: s.session });
  return [...(s.session ? [chat(s.session)] : []), logs(lines)];
}

export default step({
  name: "verify",
  label: "Verify",
  icon: "sTest",
  doing: "Verifying",
  view: shown,
  async run(ctx, opts) {
    if (opts.criterion === undefined && !opts.agent) return ctx.fail({ error: "verify needs a criterion or an agent" });
    const results = await checked(ctx, opts);
    const summary = results.map(lineOf).join("\n");
    const unmet = results.filter((result) => !result.met);
    if (unmet.length > 0) return ctx.fail({ results, summary, error: unmet.map(lineOf).join("\n") });
    if (!opts.agent) return ctx.pass({ results, summary, met: true });
    const verdict = await byAgent(ctx, opts, results);
    const data = { results, summary, session: verdict.session, verdict: verdict.reason, approved: verdict.approved };
    return verdict.approved ? ctx.pass({ ...data, met: true }) : ctx.fail({ ...data, error: `${opts.agent} rejected the item` });
  },
});
