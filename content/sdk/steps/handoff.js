import { step, actions, markdown } from "sdk";
import { defined, object, waitOptions } from "./context.js";
import { given } from "./given.js";

const DONE = "done";

function shown(s) {
  const parts = s.text ? [markdown(s.text)] : [];
  if (s.by) parts.push(markdown(`Done by ${s.by}${s.note ? `: ${s.note}` : ""}`));
  return parts;
}

export default step({
  name: "hand-over",
  id: "handoff",
  label: "Handoff",
  icon: "right",
  doing: "Waiting for a person",
  view: shown,
  async run(ctx, opts) {
    const text = String((await given(ctx, opts.text)) ?? `Take over: ${ctx.item.title ?? ""}`);
    const view = [markdown(text), actions([{ id: DONE, label: opts.doneLabel ?? "Done", variant: "primary" }])];
    const answer = object(await ctx.waitHuman(view, waitOptions(opts)));
    return ctx.pass(defined({ text, by: answer.by, note: object(answer.values).note }));
  },
});
