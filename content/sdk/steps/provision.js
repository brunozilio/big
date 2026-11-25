import { step, actions, chat, markdown } from "sdk";
import { agentOptions, attempt, branchOf, defined, effects, inLanguage, issueOf, object, planOf, pullRequestsOf, source, waitOptions } from "./context.js";

const NONE = "The delivered change needs no new infrastructure.";

const DONE = "provisioned";

const ANYWAY = "skip_check";

const ASK = [{ id: DONE, label: "Done, it is provisioned", icon: "check", variant: "primary" }];

const STILL = [...ASK, { id: ANYWAY, label: "Continue without checking", icon: "right" }];

function trackerOptions(ctx, opts) {
  const connection = opts.trackerConnection ?? ctx.item.connection ?? undefined;
  return { ...effects({ ...opts, connection }), ...(connection ? {} : { optional: true }) };
}

function resources(value, text) {
  return (Array.isArray(value) ? value : [])
    .map(object)
    .filter((found) => found.title)
    .map((found) => defined({ title: String(found.title), details: found[text] ? String(found[text]) : undefined }));
}

function entry(task) {
  const head = `### ${task.title}${task.issue ? ` (${task.issue})` : ""}`;
  const body = task.instructions ?? task.details;
  return body ? `${head}\n${body}` : head;
}

function discoverText(ctx, opts, checking) {
  const prs = pullRequestsOf(ctx, opts);
  const plan = planOf(ctx, opts);
  const parts = [`Find the infrastructure the delivered change of this item needs and that does not exist yet: ${ctx.item.title ?? ""}`];
  const branch = branchOf(ctx, opts);
  if (branch) parts.push(`Branch: ${branch}${prs[0]?.base ? ` (into ${prs[0].base})` : ""}`);
  if (prs.length > 0) parts.push(`Pull requests: ${prs.map((found) => found.pr).filter(Boolean).join(", ")}`);
  if (plan.infra.length > 0) parts.push(`## What the plan expected\n\n${plan.infra.map(entry).join("\n\n")}`);
  if (checking) parts.push(`## Check only these, someone said they were created\n\n${checking.map(entry).join("\n\n")}`);
  return inLanguage(ctx, parts.join("\n\n"));
}

async function discover(ctx, opts, checking) {
  const worktree = opts.worktree ?? branchOf(ctx, opts) ?? undefined;
  const options = agentOptions({ ...opts, model: opts.discoverModel, mode: undefined }, { worktree });
  const result = await ctx.agent(opts.discover ?? "infra-discover", { prompt: discoverText(ctx, opts, checking) }, options);
  return { session: result.session, missing: resources(object(result.output).missing, "details") };
}

async function provisioned(ctx, opts, missing) {
  const text = `Get this infrastructure provisioned for the item ${ctx.item.title ?? ""}.\n\n${missing.map(entry).join("\n\n")}`;
  const worktree = opts.worktree ?? branchOf(ctx, opts) ?? undefined;
  const result = await ctx.agent(opts.agent ?? "provisioner", { prompt: inLanguage(ctx, text), missing }, agentOptions(opts, { worktree }));
  const output = object(result.output);
  const done = (Array.isArray(output.done) ? output.done : []).map(String);
  const written = resources(output.manual, "instructions").map((found) => ({ title: found.title, instructions: found.details }));
  const manual = missing
    .filter((task) => !done.includes(task.title))
    .map((task) => defined({ ...task, instructions: written.find((found) => found.title === task.title)?.instructions ?? task.details }));
  return { session: result.session, done: missing.filter((task) => done.includes(task.title)), manual };
}

async function subtasks(ctx, opts, manual) {
  const parent = issueOf(ctx, opts);
  if (!parent || opts.subtasks === false) return manual;
  const project = opts.project ?? source(ctx, opts, "ticket").project ?? ctx.item.project ?? undefined;
  const made = [];
  for (const task of manual) {
    const child = defined({ project, title: task.title, description: task.instructions, parent: String(parent) });
    const found = await attempt(() => ctx.call("tracker", "create_issue", child, trackerOptions(ctx, opts)));
    made.push(defined({ ...task, issue: object(found).ref }));
  }
  return made;
}

async function closedInTracker(ctx, opts, manual) {
  const parent = issueOf(ctx, opts);
  const ours = manual.map((task) => task.issue).filter(Boolean);
  for (;;) {
    await ctx.sleep(opts.every ?? 60000);
    const found = await attempt(() => ctx.call("tracker", "issue", { issue: String(parent) }, trackerOptions(ctx, opts)));
    const listed = (Array.isArray(object(found).subtasks) ? found.subtasks : []).map(object);
    if (ours.every((ref) => listed.some((task) => task.ref === ref && task.done))) return { action: DONE, by: "tracker" };
  }
}

async function confirmed(ctx, opts, manual, lead, buttons) {
  const view = [markdown([lead, ...manual.map(entry)].join("\n\n")), actions(buttons)];
  const watch = manual.some((task) => task.issue) && buttons === ASK;
  for (;;) {
    const answer = watch
      ? (await ctx.race([(branch) => branch.waitHuman(view, waitOptions(opts)), (branch) => closedInTracker(branch, opts, manual)])).value
      : await ctx.waitHuman(view, waitOptions(opts));
    if (answer.action === DONE || answer.action === ANYWAY) return answer;
  }
}

async function close(ctx, opts, tasks) {
  const declared = ctx.workflow ? ctx.workflow.statuses : [];
  const status = opts.doneStatus ?? declared[declared.length - 1];
  if (!status) return;
  for (const task of tasks.filter((found) => found.issue)) {
    await attempt(() => ctx.call("tracker", "transition", { issue: task.issue, status }, trackerOptions(ctx, opts)));
  }
}

async function byHand(ctx, opts, manual) {
  let lead = "This infrastructure has to be provisioned by hand. Create it, then confirm.";
  let buttons = ASK;
  for (;;) {
    const answer = await confirmed(ctx, opts, manual, lead, buttons);
    if (answer.action === ANYWAY || opts.verify === false) return { by: answer.by, checked: false };
    const left = (await discover(ctx, opts, manual)).missing.map((found) => found.title);
    const pending = manual.filter((task) => left.includes(task.title));
    if (pending.length === 0) return { by: answer.by, checked: true };
    lead = `The check still misses: ${pending.map((task) => task.title).join(", ")}. Create it, then confirm again.`;
    buttons = STILL;
  }
}

function shown(s) {
  const chats = [s.discovery, s.session].filter(Boolean).map((session) => chat(session));
  const manual = Array.isArray(s.manual) && s.manual.length > 0 ? [markdown(s.manual.map(entry).join("\n\n"))] : [];
  const reason = s.reason ? [markdown(s.reason)] : [];
  return [...manual, ...reason, ...chats];
}

export default step({
  name: "provision",
  label: "Provision",
  icon: "server",
  doing: "Provisioning",
  view: shown,
  async run(ctx, opts) {
    const found = await discover(ctx, opts, null);
    if (found.missing.length === 0) return ctx.skip({ reason: NONE, discovery: found.session });
    const ran = await provisioned(ctx, opts, found.missing);
    const data = defined({ discovery: found.session, session: ran.session, missing: found.missing, done: ran.done });
    if (ran.manual.length === 0) return ctx.pass(data);
    const manual = await subtasks(ctx, opts, ran.manual);
    const answer = await byHand(ctx, opts, manual);
    await close(ctx, opts, manual);
    return ctx.pass(defined({ ...data, manual, by: answer.by, checked: answer.checked }));
  },
});
