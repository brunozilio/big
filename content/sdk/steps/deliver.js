import { step, markdown } from "sdk";
import { performed, settled, shown } from "./act.js";
import { draftOf } from "./given.js";

export default step({
  name: "deliver",
  label: "Deliver",
  icon: "send",
  doing: "Delivering",
  view: (s) => (s.result === undefined && s.draft ? [markdown(s.draft)] : shown(s)),
  async run(ctx, opts) {
    const { text } = draftOf(ctx, opts, "draft");
    const args = opts.args ?? { title: ctx.item.title ?? "", body: text };
    const kept = {};
    const result = await performed(ctx, { ...opts, args }, kept);
    if (result === kept) return ctx.pass({ draft: text });
    return settled(ctx, opts, result);
  },
});
