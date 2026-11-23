import { step, markdown } from "sdk";
import { branchOf, defined, effects, issueOf, object, planOf } from "./context.js";

function describe(plan, issue) {
  const parts = [];
  if (issue) parts.push(`Issue: ${issue}`);
  if (plan.text) parts.push(plan.text);
  return parts.join("\n\n");
}

export default step({
  name: "pr",
  label: "Pull request",
  icon: "sPr",
  needs: "code",
  doing: "Abrindo PR",
  view: (s) => (s.title ? [markdown(`# ${s.title}\n\n${s.body ?? ""}`)] : []),
  async run(ctx, opts) {
    const previous = object(ctx.output(ctx.step));
    if (previous.pr) return ctx.pass(previous);
    const head = branchOf(ctx, opts);
    if (!head) return ctx.fail({ error: "não há branch para abrir o pull request" });
    const plan = planOf(ctx, opts);
    const body = opts.body ?? describe(plan, issueOf(ctx, opts));
    const args = defined({ head, base: opts.base, title: plan.title, body, draft: opts.draft });
    const opened = await ctx.call("code", "open_pull_request", args, effects(opts));
    return ctx.pass({
      pr: opened.ref,
      number: opened.number ?? null,
      url: opened.url ?? null,
      title: opened.title ?? plan.title,
      body: opened.body ?? body,
      head: opened.head ?? head,
      base: opened.base ?? opts.base ?? null,
    });
  },
});
