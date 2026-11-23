import { step, issue } from "sdk";
import { defined, effects, object, planOf } from "./context.js";

export default step({
  name: "ticket",
  view: (s) => (s.issue ? [issue(s.issue)] : []),
  async run(ctx, opts) {
    const existing = opts.issue ?? ctx.item.issue ?? object(ctx.output(ctx.step)).issue;
    if (existing) return ctx.pass({ issue: existing, created: false });
    const plan = planOf(ctx, opts);
    const args = defined({
      project: opts.project,
      title: plan.title,
      description: opts.description ?? plan.text,
      type: opts.type,
      labels: opts.labels,
      parent: opts.parent,
    });
    const created = await ctx.call("tracker", "create_issue", args, effects(opts));
    return ctx.pass(defined({ issue: created.ref, url: created.url, title: created.title ?? plan.title, created: true }));
  },
});
