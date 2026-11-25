import { step } from "sdk";
import { environments, initialStages, accepting, changeEnvs, canCreateChange, canPromote, canRollback, tasksLocked, delivered, current, WAITING } from "./rules.js";
import { defaultTasks, addTask, refreshServices, schedule, sanitize, unshifted } from "./tasks.js";
import { services, promote, rollback, createChange, settleChange } from "./gates.js";
import { publish, changeArgs } from "./publish.js";
import { render } from "./view.js";

export { environments, render };

function fresh(config, item) {
  return {
    title: item.title || (config.labels && config.labels.title) || "Package",
    members: [],
    stages: initialStages(config.environments),
    windows: {},
    changes: {},
    versions: {},
    merges: {},
    merged: {},
    memberShas: {},
    tasks: [],
    services: [],
    card: null,
    cardLinks: [],
    reverting: null,
    taskSeq: 0,
  };
}

function remember(state, member) {
  if (!state.application) state.application = member.application || (member.input && member.input.application) || null;
  const sha = ((member.steps || []).find((run) => run.id === "merge" && run.output) || {}).output;
  const repo = Object.keys(member.git || {})[0];
  if (sha && sha.sha && repo) state.memberShas[repo] = [...(state.memberShas[repo] || []), sha.sha];
  state.members.push({ id: member.id, title: member.title, issue: member.issue || null, git: member.git || {}, sources: member.sources || [] });
}

async function gather(ctx, state, config, item, ids) {
  if (ids.length === 0) return false;
  for (const id of ids) await ctx.link(id);
  for (const member of await ctx.items(ids)) remember(state, member);
  state.services = services(state, config, item.application || state.application);
  if (state.tasks.length === 0) state.tasks = defaultTasks(state, config, state.services);
  else refreshServices(state, config, state.services);
  const merge = config.environments.find((env) => env.gate.kind === "merge" && env.autoPromote);
  if (merge && state.stages[merge.id] !== "pending") await promote(ctx, state, config, merge, item.application || state.application);
  return true;
}

async function drop(ctx, state, config, item, id) {
  if (!accepting(config.environments, state)) return false;
  await ctx.unlink(id);
  state.members = state.members.filter((member) => member.id !== id);
  state.services = services(state, config, item.application || state.application);
  refreshServices(state, config, state.services);
  return true;
}

const envOf = (config, id) => config.environments.find((env) => env.id === id);

async function act(ctx, state, config, item, said) {
  const values = said.values || {};
  const env = envOf(config, values.env);
  const envs = config.environments;
  switch (said.action) {
    case "link":
      return accepting(envs, state) ? gather(ctx, state, config, item, (values.items || [values.item]).filter((id) => id && !state.members.some((member) => member.id === id))) : false;
    case "unlink":
      return drop(ctx, state, config, item, values.item);
    case "window":
      if (!env || env.gate.kind !== "change" || state.changes[env.id]) return false;
      state.windows[env.id] = String(values.at || "");
      schedule(state, config);
      return true;
    case "promote":
      if (!env || !canPromote(state, env)) return false;
      await promote(ctx, state, config, env, item.application || state.application);
      return true;
    case "rollback":
      if (!env || !canRollback(envs, state, env)) return false;
      await rollback(ctx, state, config, env, item.application || state.application);
      return true;
    case "create_change":
      if (!env || !canCreateChange(envs, state, env)) return false;
      if (config.agents && config.agents.validate) await ctx.agent(config.agents.validate, { package: changeArgs(state, config, env.id), environment: env.id });
      await createChange(ctx, state, config, env, changeArgs(state, config, env.id));
      if (!accepting(envs, state)) await ctx.accept(false);
      return true;
    case "add_task": {
      if (tasksLocked(envs, state)) return false;
      const task = addTask(state, config, values.template);
      if (task) state.tasks = [task, ...state.tasks];
      return Boolean(task);
    }
    case "tasks":
      if (tasksLocked(envs, state)) return false;
      state.tasks = unshifted(state, config, (current(envs, state) || {}).id, sanitize(values.tasks, config));
      return true;
    default:
      return false;
  }
}

function waits(ctx, state, config, manual) {
  const list = [];
  if (!manual && accepting(config.environments, state)) list.push(ctx.event("item.step.finished", (config.arrival && config.arrival.filter) || {}));
  for (const env of changeEnvs(config.environments)) {
    const change = state.changes[env.id];
    if (!change || ["done", "cancelled", "rejected"].includes(change.status)) continue;
    list.push(ctx.event("changes.status.changed", { subject: change.ref }));
    list.push(ctx.sleep((config.monitor && config.monitor.everyMs) || 60000));
  }
  return list;
}

async function monitor(ctx, state, config) {
  let moved = false;
  for (const env of changeEnvs(config.environments)) {
    const change = state.changes[env.id];
    if (!change || ["done", "cancelled", "rejected"].includes(change.status)) continue;
    const read = await ctx.call("changes", "get", { change: change.ref });
    if (settleChange(state, config, env, read.status)) moved = true;
  }
  return moved;
}

async function promotion(ctx, opts) {
  const config = { ...opts, environments: environments(opts) };
  const item = ctx.item;
  const state = fresh(config, item);
  const manual = !item.event;
  for (;;) {
    let changed = false;
    if (!manual && accepting(config.environments, state)) changed = await gather(ctx, state, config, item, await ctx.unclaimed(config.arrival.step || "merge", "passed", { base: config.arrival.base }));
    if (changed) await publish(ctx, state, config);
    if (delivered(config.environments, state)) return ctx.pass({ changes: state.changes, members: state.members.map((member) => member.id) });
    const answer = await ctx.race([...waits(ctx, state, config, manual), (branch) => branch.waitHuman(render(config, state))]);
    const said = answer.value && answer.value.action ? answer.value : null;
    const moved = said ? await act(ctx, state, config, item, said) : await monitor(ctx, state, config);
    if (moved) await publish(ctx, state, config);
  }
}

export const release = step({
  name: "promotion",
  label: "Promotion",
  icon: "package",
  run: promotion,
});

export { WAITING };
