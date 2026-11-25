import { step, chat } from "sdk";
import { agentOptions, defined, inLanguage, languageOf, object, parsePlan } from "./context.js";

function request(ctx, opts) {
  const item = ctx.item;
  if (opts.prompt) return opts.prompt;
  const parts = [`Plan the delivery of this item: ${item.title ?? ""}`];
  if (item.issue) parts.push(`Issue: ${item.issue}`);
  if (item.description) parts.push(String(item.description));
  return inLanguage(ctx, parts.join("\n\n"));
}

export default step({
  name: "plan",
  label: "Plan",
  icon: "sPlan",
  doing: "Planning",
  view: (s) => (s.session ? [chat(s.session, { composer: true })] : []),
  async run(ctx, opts) {
    const item = ctx.item;
    const previous = object(ctx.output(ctx.step));
    const input = defined({ prompt: request(ctx, opts), title: item.title, issue: item.issue, session: previous.session });
    const worktree = opts.worktree ?? item.branch ?? undefined;
    const result = await ctx.agent(opts.agent ?? "planner", input, agentOptions(opts, { mode: opts.mode ?? "plan", worktree }));
    const output = object(result.output);
    const written = parsePlan(output.plan ?? result.plan ?? result.text ?? "", languageOf(ctx));
    const plan = written.text;
    if (output.approved === false) {
      return ctx.fail({ session: result.session, plan, error: output.reason ?? "the plan was not approved" });
    }
    const title = output.title ?? (written.title || item.title || "");
    return ctx.pass({ session: result.session, plan, title, story: written.story, subtasks: written.subtasks, infra: written.infra });
  },
});
