import { step, change, markdown } from "sdk";
import { branchOf, codeOptions, codeTargets, defined, eachOf, effects, issueOf, object, planOf, repoOf, source, summaryLine, targetFields } from "./context.js";

function describe(summary, plan, issue) {
  const parts = [summary || summaryLine(plan.story) || plan.title];
  if (issue) parts.push(`Issue: ${issue}`);
  return parts.filter(Boolean).join("\n\n");
}

function titled(plan, issue) {
  return issue ? `${issue} - ${plan.title}` : plan.title;
}

function shown(entry, many) {
  const title = many && entry.name ? `${entry.name}: ${entry.title}` : entry.title;
  return markdown(`# ${title}\n\n${entry.body ?? ""}`);
}

function opened(entries) {
  const many = entries.length > 1;
  return entries.filter((entry) => entry.title || entry.pr).map((entry) => (entry.pr ? change(entry.pr) : shown(entry, many)));
}

async function open(ctx, opts, target, wanted) {
  const options = codeOptions(opts, target);
  const args = defined({ head: wanted.head, base: opts.base, repo: repoOf(target), title: wanted.title, body: wanted.body, draft: opts.draft });
  const made = await ctx.call("code", "open_pull_request", args, effects(options));
  return {
    ...targetFields(target),
    pr: made.ref,
    number: made.number ?? null,
    url: made.url ?? null,
    title: made.title ?? wanted.title,
    body: made.body ?? wanted.body,
    head: made.head ?? wanted.head,
    base: made.base ?? opts.base ?? null,
  };
}

export default step({
  name: "pr",
  label: "Pull request",
  icon: "sPr",
  needs: "code",
  doing: "Abrindo PR",
  view: (s) => opened(Array.isArray(s.prs) && s.prs.length > 0 ? s.prs : [s]),
  async run(ctx, opts) {
    const previous = object(ctx.output(ctx.step));
    if (previous.pr) return ctx.pass(previous);
    const head = branchOf(ctx, opts);
    if (!head) return ctx.fail({ error: "não há branch para abrir o pull request" });
    const targets = codeTargets(ctx, opts);
    if (targets.length === 0) return ctx.fail({ error: "nenhum repositório da aplicação tem conexão de código" });
    const plan = planOf(ctx, opts);
    const issue = issueOf(ctx, opts);
    const summary = summaryLine(source(ctx, opts, "develop").summary);
    const body = opts.body ?? describe(summary, plan, issue);
    const wanted = { head, title: opts.title ?? titled(plan, issue), body };
    const prs = await eachOf(ctx, targets, (branch, target) => open(branch, opts, target, wanted));
    const { repository: _repository, name: _name, code: _code, ...data } = prs[0];
    return ctx.pass(targets.length > 1 || targets[0].id ? { ...data, prs } : data);
  },
});
