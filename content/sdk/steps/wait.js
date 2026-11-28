import { step, actions, markdown } from "sdk";
import { defined, object, waitOptions } from "./context.js";
import { given } from "./given.js";

const CONTINUE = "continue";

async function waits(ctx, opts) {
  const found = [];
  if (typeof opts.event === "string" && opts.event) found.push(ctx.event(opts.event, (await given(ctx, opts.filter)) ?? {}));
  const polled = object(opts.poll);
  if (polled.capability && polled.action) {
    const call = defined({ capability: polled.capability, action: polled.action, args: (await given(ctx, polled.args)) ?? {}, connection: polled.connection });
    found.push(ctx.poll(call, polled.every ?? 60000, defined({ until: polled.until })));
  }
  if (opts.human) {
    const text = String((await given(ctx, opts.human)) ?? "");
    found.push(ctx.waitHuman([markdown(text), actions([{ id: CONTINUE, label: opts.continueLabel ?? "Continue", variant: "primary" }])], waitOptions(opts)));
  }
  if (Number(opts.ms) > 0) found.push(ctx.sleep(Number(opts.ms)));
  return found;
}

export default step({
  name: "wait",
  label: "Wait",
  icon: "hourglass",
  doing: "Waiting",
  view: (s) => (s.reason ? [markdown(s.reason)] : []),
  async run(ctx, opts) {
    if (opts.onReturn === true && !ctx.from) return ctx.skip({ reason: "Nothing to wait for on the first pass." });
    const pending = await waits(ctx, opts);
    if (pending.length === 0) return ctx.fail({ error: "wait needs ms, event, poll or human" });
    const raced = await ctx.race(pending);
    const by = raced.kind === "sleep" ? "time" : raced.kind;
    return ctx.pass(defined({ by, value: raced.value ?? undefined }));
  },
});
