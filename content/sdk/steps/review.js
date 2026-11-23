import { step, actions, chat, review as changes } from "sdk";
import { agentOptions, defined, effects, object, planOf, pullRequestOf, waitOptions } from "./context.js";

const VERDICTS = ["approve", "request_changes", "comment"];

async function automatic(ctx, opts, pr) {
  const plan = planOf(ctx, opts);
  const prompt = `Revise o pull request ${pr}.\n\n## Plano\n\n${plan.text}`;
  const result = await ctx.agent(opts.agent, { prompt, pr }, agentOptions(opts, { mode: opts.mode ?? "plan" }));
  const output = object(result.output);
  const lines = Array.isArray(output.lines) ? output.lines : [];
  if (lines.length > 0 || output.body) {
    const args = defined({ pr, verdict: "comment", body: output.body, lines });
    await ctx.call("code", "submit_review", args, effects(opts));
  }
  return result.session;
}

async function changesAsked(ctx, opts, pr, answer) {
  const values = object(answer.values);
  const threads = await ctx.call("code", "review_comments", { pr }, effects(opts));
  const comments = (Array.isArray(threads) ? threads : [])
    .filter((entry) => !entry.resolved)
    .map((entry) => defined({ path: entry.path, line: entry.line, body: entry.body, author: entry.author }));
  return { pr, verdict: "request_changes", body: values.body ?? null, comments, by: answer.by ?? null };
}

export default step({
  name: "review",
  view: (s) => [...(s.pr ? [changes(s.pr)] : []), ...(s.session ? [chat(s.session)] : [])],
  async run(ctx, opts) {
    const pr = pullRequestOf(ctx, opts)?.pr;
    if (!pr) return ctx.fail({ error: "não há pull request para revisar" });
    const session = opts.agent ? await automatic(ctx, opts, pr) : undefined;
    for (;;) {
      const answer = await ctx.waitHuman([changes(pr), actions(opts.actions ?? VERDICTS)], waitOptions(opts));
      const values = object(answer.values);
      const lines = Array.isArray(values.lines) ? values.lines : [];
      if (!VERDICTS.includes(answer.action)) continue;
      if (answer.action === "comment" && !values.body && lines.length === 0) continue;
      const args = defined({ pr, verdict: answer.action, body: values.body, lines: lines.length > 0 ? lines : undefined });
      await ctx.call("code", "submit_review", args, effects(opts));
      if (answer.action === "approve") return ctx.pass(defined({ pr, verdict: "approve", by: answer.by, session }));
      if (answer.action === "request_changes") {
        const data = { ...(await changesAsked(ctx, opts, pr, answer)), ...defined({ session }) };
        const target = opts.onChanges === undefined ? "develop" : opts.onChanges;
        return target ? ctx.goto(target, data) : ctx.pass(data);
      }
    }
  },
});
