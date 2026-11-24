import { step, chat, markdown } from "sdk";
import { agentOptions, attempt, branchOf, codeOptions, defined, effects, feedbackText, inLanguage, issueOf, object, planOf, pullRequestsOf, source, summaryLine } from "./context.js";

const IDLE = "Nothing to fix: the pull request has no pending review comments or reported failures.";

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
  return { step: "review", outcome: "comments", data: { summary: "The pull request has pending review comments.", comments } };
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
  const parts = [`Implement the item: ${plan.title || item.title || ""}`];
  if (issue) parts.push(`Issue: ${issue}`);
  if (plan.text) parts.push(`## Approved plan\n\n${plan.text}`);
  const back = feedbackText(feedback);
  if (back) parts.push(`## What needs fixing\n\n${back}`);
  return inLanguage(parts.join("\n\n"));
}

function uncommitted(result, branch) {
  const trees = Array.isArray(result.repositories) && result.repositories.length > 0 ? result.repositories : [result];
  const known = trees.filter((tree) => tree && typeof tree === "object" && "commit" in tree);
  if (known.length === 0 || known.some((tree) => tree.commit && tree.commit !== tree.base)) return null;
  if (known.every((tree) => !tree.commit)) return "the working folder is not a git repository, so nothing was committed or pushed";
  return `the agent finished without any commit on branch ${branch}`;
}

const NUDGE = "You stopped without any commit. Continue the work now: implement the plan, run the tests and commit on the item's branch. Do not stop before there are commits.";

function stalled(result) {
  const trees = Array.isArray(result.repositories) && result.repositories.length > 0 ? result.repositories : [result];
  const known = trees.filter((tree) => tree && typeof tree === "object" && tree.commit);
  return known.length > 0 && known.every((tree) => tree.commit === tree.base);
}

function trackerOptions(ctx, opts) {
  const connection = opts.trackerConnection ?? ctx.item.connection ?? undefined;
  return { ...effects({ ...opts, connection }), ...(connection ? {} : { optional: true }) };
}

function detailsOf(plan, title) {
  return plan.subtasks.find((task) => task.title === title)?.details ?? "";
}

async function subtasksOf(ctx, opts, issue) {
  if (opts.subtasks === false) return [];
  const plan = planOf(ctx, opts);
  const made = source(ctx, opts, "ticket").subtasks;
  if (Array.isArray(made) && made.length > 0) {
    return made.filter((task) => task && task.issue).map((task) => ({ issue: task.issue, title: task.title ?? "", details: detailsOf(plan, task.title) }));
  }
  if (!issue) return [];
  const found = await attempt(() => ctx.call("tracker", "issue", { issue: String(issue) }, trackerOptions(ctx, opts)));
  const listed = Array.isArray(object(found).subtasks) ? found.subtasks : [];
  return listed.map(object).filter((task) => task.ref && !task.done).map((task) => ({ issue: task.ref, title: task.title ?? "", details: detailsOf(plan, task.title) }));
}

function statusesOf(ctx, opts) {
  const declared = ctx.workflow ? ctx.workflow.statuses : [];
  return { doing: opts.subtaskDoing ?? opts.status, done: opts.subtaskDone ?? declared[declared.length - 1] };
}

async function move(ctx, opts, issue, status) {
  if (!status) return;
  await attempt(() => ctx.call("tracker", "transition", { issue, status }, trackerOptions(ctx, opts)));
}

function subtaskText(task, index, total) {
  const parts = [`Now do only subtask ${index + 1} of ${total}: ${task.title} (${task.issue}).`];
  if (task.details) parts.push(task.details);
  parts.push("When this subtask is done, commit and stop; the next one comes right after.");
  return parts.join("\n\n");
}

async function stepwise(ctx, opts, run, tasks) {
  const statuses = statusesOf(ctx, opts);
  const lead = "Work on one subtask at a time, in the plan's order.";
  let result = null;
  for (const [index, task] of tasks.entries()) {
    await move(ctx, opts, task.issue, statuses.doing);
    const text = subtaskText(task, index, tasks.length);
    const input = result ? { prompt: inLanguage(text), session: result.session } : { ...run.input, prompt: `${run.input.prompt}\n\n${lead}\n\n${text}` };
    result = await ctx.agent(run.agent, input, run.options);
    await move(ctx, opts, task.issue, statuses.done);
  }
  return result;
}

export default step({
  name: "develop",
  label: "Develop",
  icon: "sCode",
  doing: "Developing",
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
    const run = { agent, input, options: agentOptions(opts, { worktree }) };
    const tasks = feedback || previous.session || opts.prompt ? [] : await subtasksOf(ctx, opts, issue);
    let result = tasks.length > 0 ? await stepwise(ctx, opts, run, tasks) : await ctx.agent(agent, input, run.options);
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
