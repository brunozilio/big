import { step, change, markdown } from "sdk";
import { branchOf, codeOptions, codeTargets, defined, eachOf, effects, gitOf, issueOf, object, planOf, repoOf, source, sourcePullRequestOf, summaryLine, targetFields } from "./context.js";

function describe(summary, plan, issue, extra) {
  const parts = [summary || summaryLine(plan.story) || plan.title];
  if (issue) parts.push(`Issue: ${issue}`);
  if (extra) parts.push(extra);
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

function wantedOf(ctx, opts, target, wanted) {
  const git = gitOf(ctx, target.id);
  return { ...wanted, head: opts.branch ?? git.head ?? wanted.head, base: opts.base ?? git.base ?? undefined };
}

async function reuse(ctx, opts, target, found, wanted) {
  const options = codeOptions(found.connection ? { ...opts, connection: opts.connection ?? found.connection } : opts, target);
  const known = object(await ctx.call("code", "pull_request", { pr: found.subject }, effects(options)));
  return {
    ...targetFields(target),
    pr: found.subject,
    number: known.number ?? null,
    url: known.url ?? null,
    title: known.title ?? wanted.title,
    body: known.body ?? wanted.body,
    head: known.head ?? wanted.head,
    base: known.base ?? wanted.base ?? null,
    reused: true,
  };
}

async function open(ctx, opts, target, wanted) {
  const options = codeOptions(opts, target);
  const args = defined({ head: wanted.head, base: wanted.base, repo: repoOf(target), title: wanted.title, body: wanted.body, draft: opts.draft });
  const made = await ctx.call("code", "open_pull_request", args, effects(options));
  return {
    ...targetFields(target),
    pr: made.ref,
    number: made.number ?? null,
    url: made.url ?? null,
    title: made.title ?? wanted.title,
    body: made.body ?? wanted.body,
    head: made.head ?? wanted.head,
    base: made.base ?? wanted.base ?? null,
  };
}

function targetsOf(ctx, opts) {
  const targets = codeTargets(ctx, opts);
  if (Array.isArray(source(ctx, opts, "develop").repositories)) return targets;
  const reused = targets.filter((target) => sourcePullRequestOf(ctx, target));
  return reused.length > 0 ? reused : targets;
}

async function deliver(ctx, opts, target, wanted) {
  const own = wantedOf(ctx, opts, target, wanted);
  const found = sourcePullRequestOf(ctx, target);
  if (found) return reuse(ctx, opts, target, found, own);
  return open(ctx, opts, target, own);
}

export default step({
  name: "pr",
  label: "Pull request",
  icon: "sPr",
  requires: "code",
  doing: "Opening PR",
  view: (s) => opened(Array.isArray(s.prs) && s.prs.length > 0 ? s.prs : [s]),
  async run(ctx, opts) {
    const previous = object(ctx.output(ctx.step));
    if (previous.pr) return ctx.pass(previous);
    const head = branchOf(ctx, opts);
    const targets = targetsOf(ctx, opts);
    if (targets.length === 0) return ctx.fail({ error: "no repository of the application has a code connection" });
    const known = targets.every((target) => gitOf(ctx, target.id).head || sourcePullRequestOf(ctx, target));
    if (!head && !known) return ctx.fail({ error: "there is no branch to open the pull request" });
    const plan = planOf(ctx, opts);
    const issue = issueOf(ctx, opts);
    const summary = summaryLine(source(ctx, opts, "develop").summary);
    const body = opts.body ?? describe(summary, plan, issue, source(ctx, opts, "plan").pullRequestBody);
    const wanted = { head, title: opts.title ?? titled(plan, issue), body };
    const prs = await eachOf(ctx, targets, (branch, target) => deliver(branch, opts, target, wanted));
    const { repository: _repository, name: _name, code: _code, ...data } = prs[0];
    return ctx.pass(targets.length > 1 || targets[0].id ? { ...data, prs } : data);
  },
});
