import { step, actions, markdown, review as changes } from "sdk";
import { agentOptions, codeOptions, defined, effects, inLanguage, object, planOf, pullRequestsOf, waitOptions } from "./context.js";

const VERDICTS = ["approve", "request_changes", "comment"];

export function verdictOf(text) {
  const written = String(text ?? "");
  const at = written.search(/^#{1,3}\s*(veredito|resumo|verdict|summary)\b/im);
  return at < 0 ? "" : written.slice(at).trim();
}

async function automatic(ctx, opts, pr) {
  const plan = planOf(ctx, opts);
  const prompt = inLanguage(`Revise o pull request ${pr}.\n\n## Plano\n\n${plan.text}`);
  const result = await ctx.agent(opts.agent, { prompt, pr }, agentOptions(opts, { mode: opts.mode ?? "plan" }));
  const output = object(result.output);
  const lines = Array.isArray(output.lines) ? output.lines : [];
  const body = output.body ?? (lines.length > 0 ? undefined : verdictOf(result.text));
  if (lines.length > 0 || body) {
    const args = defined({ pr, verdict: "comment", body, lines: lines.length > 0 ? lines : undefined });
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

async function judge(ctx, opts, entry, many, session) {
  const pr = entry.pr;
  const view = [
    ...(many && entry.name ? [markdown(`**${entry.name}**`)] : []),
    changes(pr, { session }),
    ...(opts.actions ? [actions(opts.actions)] : []),
  ];
  for (;;) {
    const answer = await ctx.waitHuman(view, waitOptions(opts));
    const values = object(answer.values);
    const lines = Array.isArray(values.lines) ? values.lines : [];
    if (!VERDICTS.includes(answer.action)) continue;
    if (answer.action === "comment" && !values.body && lines.length === 0) continue;
    const args = defined({ pr, verdict: answer.action, body: values.body, lines: lines.length > 0 ? lines : undefined });
    await ctx.call("code", "submit_review", args, effects(opts));
    if (answer.action === "approve") return { approved: true, by: answer.by };
    if (answer.action === "request_changes") {
      return { approved: false, data: { ...(await changesAsked(ctx, opts, pr, answer)), ...defined({ session }) } };
    }
  }
}

export default step({
  name: "review",
  label: "Review",
  icon: "sReview",
  needs: "code",
  doing: "Esperando review",
  view: (s) => (Array.isArray(s.prs) ? s.prs : s.pr ? [s.pr] : []).map((pr) => changes(pr, { session: s.session })),
  async run(ctx, opts) {
    const prs = pullRequestsOf(ctx, opts);
    if (prs.length === 0) return ctx.fail({ error: "não há pull request para revisar" });
    let session;
    let by;
    for (const entry of prs) {
      const options = codeOptions(opts, entry);
      if (opts.agent) session = await automatic(ctx, options, entry.pr);
      const judged = await judge(ctx, options, entry, prs.length > 1, session);
      if (!judged.approved) {
        const target = opts.onChanges === undefined ? "develop" : opts.onChanges;
        return target ? ctx.goto(target, judged.data) : ctx.pass(judged.data);
      }
      by = judged.by;
    }
    const listed = prs.length > 1 ? { prs: prs.map((entry) => entry.pr) } : {};
    return ctx.pass(defined({ pr: prs[0].pr, verdict: "approve", by, session, ...listed }));
  },
});
