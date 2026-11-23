import { step, issue } from "sdk";
import { defined, effects, object, planOf } from "./context.js";

export default step({
  name: "ticket",
  label: "Issue",
  icon: "sTicket",
  needs: "tracker",
  doing: "Criando issue",
  view: (s) => (s.issue ? [issue(s.issue)] : []),
  async run(ctx, opts) {
    const existing = opts.issue ?? ctx.item.issue ?? object(ctx.output(ctx.step)).issue;
    if (existing) return ctx.pass({ issue: existing, created: false });
    const plan = planOf(ctx, opts);
    const args = defined({
      project: opts.project ?? ctx.item.project,
      title: plan.title,
      description: opts.description ?? (plan.story || plan.text),
      type: opts.type,
      labels: opts.labels,
      parent: opts.parent,
    });
    const connection = opts.connection ?? ctx.item.connection;
    const created = await ctx.call("tracker", "create_issue", args, effects({ ...opts, connection }));
    const subtasks = [];
    for (const task of opts.subtasks === false ? [] : plan.subtasks) {
      const child = defined({ project: args.project, title: task.title, description: task.details, parent: created.ref });
      const made = await ctx.call("tracker", "create_issue", child, effects({ ...opts, connection }));
      subtasks.push({ issue: made.ref, title: task.title });
    }
    return ctx.pass(defined({ issue: created.ref, url: created.url, title: created.title ?? plan.title, created: true, subtasks: subtasks.length > 0 ? subtasks : undefined }));
  },
});
