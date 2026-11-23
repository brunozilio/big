import { step, chat } from "sdk";
import { agentOptions, branchOf, defined, feedbackText, inLanguage, issueOf, object, planOf, summaryLine } from "./context.js";

function request(item, plan, issue, feedback) {
  const parts = [`Implemente o item: ${plan.title || item.title || ""}`];
  if (issue) parts.push(`Issue: ${issue}`);
  if (plan.text) parts.push(`## Plano aprovado\n\n${plan.text}`);
  const back = feedbackText(feedback);
  if (back) parts.push(`## O que precisa ser corrigido\n\n${back}`);
  return inLanguage(parts.join("\n\n"));
}

function uncommitted(result, branch) {
  const trees = Array.isArray(result.repositories) && result.repositories.length > 0 ? result.repositories : [result];
  const known = trees.filter((tree) => tree && typeof tree === "object" && "commit" in tree);
  if (known.length === 0 || known.some((tree) => tree.commit && tree.commit !== tree.base)) return null;
  if (known.every((tree) => !tree.commit)) return "a pasta de trabalho não é um repositório git, então nada foi commitado nem enviado";
  return `o agente terminou sem nenhum commit na branch ${branch}`;
}

const NUDGE = "Você encerrou sem nenhum commit. Continue o trabalho agora: implemente o plano, rode os testes e faça commit na branch do item. Não pare antes de ter commits.";

function stalled(result) {
  const trees = Array.isArray(result.repositories) && result.repositories.length > 0 ? result.repositories : [result];
  const known = trees.filter((tree) => tree && typeof tree === "object" && tree.commit);
  return known.length > 0 && known.every((tree) => tree.commit === tree.base);
}

export default step({
  name: "develop",
  label: "Develop",
  icon: "sCode",
  doing: "Desenvolvendo",
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
    const agent = opts.agent ?? "developer";
    let result = await ctx.agent(agent, input, agentOptions(opts, { worktree }));
    for (let nudged = 0; nudged < (opts.nudges ?? 2) && stalled(result); nudged += 1) {
      result = await ctx.agent(agent, { prompt: NUDGE, session: result.session }, agentOptions(opts, { worktree }));
    }
    const output = object(result.output);
    const missing = uncommitted(result, output.branch ?? branch);
    if (missing) return ctx.fail({ error: missing, session: result.session });
    const repositories = Array.isArray(result.repositories) && result.repositories.some((repository) => repository.id) ? result.repositories : undefined;
    return ctx.pass(defined({ session: result.session, branch: output.branch ?? branch, summary: output.summary ?? summaryLine(result.text), repositories }));
  },
});
