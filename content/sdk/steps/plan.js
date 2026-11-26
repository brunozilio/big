import { step, chat } from "sdk";
import { agentOptions, defined, inLanguage, issueBrief, issueOf, languageOf, object, parsePlan, readIssue } from "./context.js";

const UNREAD = "The issue could not be read from the tracker. Read it with your tracker tools before planning, and ask only what the issue does not answer.";

const READ = "This is the issue from the tracker, already read for you. Treat its title, description and comments as the item's scope; do not ask for them again.";

function request(ctx, opts, issue, found) {
  const item = ctx.item;
  if (opts.prompt) return opts.prompt;
  const parts = [`Plan the delivery of this item: ${item.title ?? ""}`];
  const brief = issueBrief(found);
  if (brief) parts.push(READ, brief);
  else if (issue) parts.push(`Issue in the tracker: ${issue}`, UNREAD);
  if (item.description && String(item.description).trim() !== String(issue ?? "").trim()) parts.push(String(item.description));
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
    const issue = issueOf(ctx, opts);
    const found = previous.session || opts.prompt ? null : await readIssue(ctx, opts, issue);
    const input = defined({ prompt: request(ctx, opts, issue, found), title: item.title, issue: issue ?? undefined, session: previous.session });
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
