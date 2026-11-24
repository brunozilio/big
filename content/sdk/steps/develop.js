import { step, chat, markdown } from "sdk";
import { agentOptions, branchOf, codeOptions, defined, effects, feedbackText, inLanguage, issueOf, object, planOf, pullRequestsOf, summaryLine } from "./context.js";

const IDLE = "Nada a corrigir: o pull request não tem comentários de review pendentes nem falhas apontadas.";

async function pending(ctx, opts) {
  const comments = [];
  for (const entry of pullRequestsOf(ctx, opts)) {
    const threads = await ctx.call("code", "review_comments", { pr: entry.pr }, effects(codeOptions(opts, entry)));
    for (const found of Array.isArray(threads) ? threads : []) {
      if (!found.resolved && !found.outdated) {
        comments.push(defined({ pr: entry.pr, thread: found.thread, path: found.path, line: found.line, body: found.body, author: found.author }));
      }
    }
  }
  if (comments.length === 0) return null;
  return { step: "review", outcome: "comments", data: { summary: "O pull request tem comentários de review pendentes.", comments } };
}

export function threadsOf(feedback, before) {
  const data = object(object(feedback).data);
  const comments = Array.isArray(data.comments) ? data.comments : [];
  const threads = (Array.isArray(before) ? before : []).filter((entry) => entry && entry.pr && entry.thread);
  for (const comment of comments.map(object)) {
    const pr = comment.pr ?? data.pr;
    if (!comment.thread || !pr || threads.some((known) => known.thread === comment.thread)) continue;
    threads.push({ pr, thread: comment.thread });
  }
  return threads;
}

function shown(s) {
  if (s.session) return [chat(s.session)];
  return s.idle ? [markdown(s.idle)] : [];
}

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
  view: shown,
  async run(ctx, opts) {
    const plan = planOf(ctx, opts);
    const issue = issueOf(ctx, opts);
    const given = opts.feedback ?? ctx.from;
    const feedback = given || !opts.feedbackOnly ? given : await pending(ctx, opts);
    if (opts.feedbackOnly && !feedback) return ctx.skip({ idle: IDLE });
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
    const threads = threadsOf(feedback, previous.threads);
    const handed = threads.length > 0 ? threads : undefined;
    const missing = uncommitted(result, output.branch ?? branch);
    if (missing) return ctx.fail(defined({ error: missing, session: result.session, threads: handed }));
    const repositories = Array.isArray(result.repositories) && result.repositories.some((repository) => repository.id) ? result.repositories : undefined;
    return ctx.pass(
      defined({
        session: result.session,
        branch: output.branch ?? branch,
        summary: output.summary ?? summaryLine(result.text),
        repositories,
        threads: handed,
      }),
    );
  },
});
