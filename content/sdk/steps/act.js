import { step, logs, markdown } from "sdk";
import { defined, effects, tail } from "./context.js";
import { given } from "./given.js";

const isOutcome = (value) => value !== null && typeof value === "object" && value.type === "outcome";

export function shown(s) {
  if (s.summary) return [markdown(s.summary)];
  if (s.error) return [markdown(s.error)];
  return s.result === undefined ? [] : [logs(tail(JSON.stringify(s.result, null, 2) ?? "null"))];
}

export async function performed(ctx, opts, fallback) {
  if (typeof opts.run === "function") return opts.run(ctx.item, ctx);
  if (typeof opts.capability !== "string" || typeof opts.action !== "string") return fallback;
  const args = (await given(ctx, opts.args)) ?? {};
  return ctx.call(opts.capability, opts.action, args, effects(opts));
}

export async function settled(ctx, opts, result) {
  if (isOutcome(result)) return result;
  const summary = typeof opts.summary === "function" ? await opts.summary(result, ctx) : opts.summary;
  return ctx.pass(defined({ capability: opts.capability, action: opts.action, result, summary }));
}

export default step({
  name: "act",
  label: "Act",
  icon: "bolt",
  doing: "Acting",
  view: shown,
  async run(ctx, opts) {
    const missing = {};
    const result = await performed(ctx, opts, missing);
    if (result === missing) return ctx.fail({ error: "act needs run, or capability and action" });
    return settled(ctx, opts, result);
  },
});
