import { step, issue, markdown } from "sdk";
import { defined, effects, object, planOf, sourcesOf } from "./context.js";

const UNTRACKED = "Nenhum rastreador de issues está conectado, então o item segue sem issue.";

function shown(s) {
  if (s.issue) return [issue(s.issue)];
  return s.reason ? [markdown(s.reason)] : [];
}

export default step({
  name: "ticket",
  label: "Issue",
  icon: "sTicket",
  needs: "tracker",
  doing: "Criando issue",
  view: shown,
  async run(ctx, opts) {
    const tracked = sourcesOf(ctx, "issue")[0];
    const existing = opts.issue ?? ctx.item.issue ?? tracked?.subject ?? object(ctx.output(ctx.step)).issue;
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
    const connection = opts.connection ?? ctx.item.connection ?? undefined;
    const options = { ...effects({ ...opts, connection }), ...(connection ? {} : { optional: true }) };
    const created = await ctx.call("tracker", "create_issue", args, options);
    if (!created) return ctx.skip({ created: false, reason: UNTRACKED });
    const subtasks = [];
    for (const task of opts.subtasks === false ? [] : plan.subtasks) {
      const child = defined({ project: args.project, title: task.title, description: task.details, parent: created.ref });
      const made = await ctx.call("tracker", "create_issue", child, options);
      subtasks.push({ issue: made.ref, title: task.title });
    }
    return ctx.pass(defined({ issue: created.ref, url: created.url, title: created.title ?? plan.title, created: true, subtasks: subtasks.length > 0 ? subtasks : undefined }));
  },
});
