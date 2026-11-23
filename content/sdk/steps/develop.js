import { step, chat } from "sdk";
import { agentOptions, branchOf, defined, feedbackText, issueOf, object, planOf } from "./context.js";

function request(item, plan, issue, feedback) {
  const parts = [`Implemente o item: ${plan.title || item.title || ""}`];
  if (issue) parts.push(`Issue: ${issue}`);
  if (plan.text) parts.push(`## Plano aprovado\n\n${plan.text}`);
  const back = feedbackText(feedback);
  if (back) parts.push(`## O que precisa ser corrigido\n\n${back}`);
  return parts.join("\n\n");
}

export default step({
  name: "develop",
  view: (s) => (s.session ? [chat(s.session)] : []),
  async run(ctx, opts) {
    const plan = planOf(ctx, opts);
    const issue = issueOf(ctx, opts);
    const feedback = opts.feedback ?? ctx.from;
    const previous = object(ctx.output(ctx.step));
    const branch = opts.branch ?? previous.branch ?? branchOf(ctx, opts);
    const input = defined({
      prompt: opts.prompt ?? request(ctx.item, plan, issue, feedback),
      plan: plan.text,
      issue,
      feedback,
      session: previous.session,
    });
    const worktree = opts.worktree ?? branch ?? ctx.item.id;
    const result = await ctx.agent(opts.agent ?? "developer", input, agentOptions(opts, { worktree }));
    const output = object(result.output);
    return ctx.pass(defined({ session: result.session, branch: output.branch ?? branch, summary: output.summary ?? result.text }));
  },
});
