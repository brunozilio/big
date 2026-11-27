import { stages, members, tasks } from "sdk";
import { canCreateChange, canDecideChange, canPromote, canRollback, changeEnvs, current, tasksLocked, visible, accepting } from "./rules.js";
import { shifted } from "./tasks.js";
import { fill } from "./gates.js";

const label = (config, name, fallback) => ((config.labels || {})[name]) || fallback;

function stageActions(config, state, env) {
  const envs = config.environments;
  const list = [];
  if (canPromote(state, env)) list.push({ id: "promote", label: label(config, "promote", "Promote"), icon: "rocket", variant: "primary", payload: { env: env.id } });
  if (canRollback(envs, state, env)) list.push({ id: "rollback", label: label(config, "rollback", "Rollback"), icon: "undo", payload: { env: env.id } });
  if (env.gate.kind === "change" && visible(envs, state, env) && !state.changes[env.id] && state.windows[env.id]) {
    list.push({ id: "create_change", label: label(config, "createChange", "Create change"), icon: "approval", variant: "primary", payload: { env: env.id }, disabled: !canCreateChange(envs, state, env) });
  }
  if (canDecideChange(state, env)) {
    list.push({ id: "approve_change", label: label(config, "approveChange", "Approve"), icon: "check", variant: "primary", payload: { env: env.id } });
    list.push({ id: "reject_change", label: label(config, "rejectChange", "Reject"), icon: "close", payload: { env: env.id } });
  }
  return list;
}

function statusLabel(config, change) {
  const said = label(config, `status_${change.status}`, change.status);
  return change.by ? `${said} · ${change.by}` : said;
}

function stageOf(config, state, env) {
  const change = state.changes[env.id];
  const shown = visible(config.environments, state, env);
  return {
    id: env.id,
    label: env.label,
    short: env.short || env.label,
    icon: env.icon || "dashedCircle",
    state: state.stages[env.id],
    gate: env.gate.kind,
    window: shown ? state.windows[env.id] || null : null,
    picker: shown && env.gate.kind === "change" && !change,
    change: change ? { key: change.key, status: change.status, label: statusLabel(config, change), url: change.url } : null,
    busy: state.stages[env.id] === "running" ? label(config, state.reverting === env.id ? "reverting" : "working", "…") : null,
    actions: stageActions(config, state, env),
  };
}

function taskKinds(config) {
  const base = [
    { kind: "deploy", icon: "rocket", tone: "info" },
    { kind: "rollback", icon: "undo", tone: "warning" },
  ];
  const extra = ((config.tasks && config.tasks.kinds) || []).map((kind) => ({ id: kind.id, kind: kind.kind, label: kind.label, icon: kind.icon || "task", tone: kind.tone || "neutral", attachable: Boolean(kind.attachable), editable: true }));
  return [...base, ...extra];
}

export function render(config, state) {
  const envs = config.environments;
  const here = current(envs, state);
  const list = shifted(state, config, here ? here.id : null).map((task) => {
    if (task.kind !== "rollback" || !here) return task;
    const version = (state.versions[here.id] || {})[task.service];
    return version ? { ...task, detail: fill(label(config, "rollbackTo", "to {version}"), { version }) } : task;
  });
  return [
    stages(envs.map((env) => stageOf(config, state, env))),
    members(state.members.map((member) => ({ id: member.id, title: member.title, issue: member.issue || null })), { removable: accepting(envs, state) }),
    tasks(list, { kinds: taskKinds(config), locked: tasksLocked(envs, state), environment: here ? here.short || here.label : null, owners: state.owners || [] }),
  ];
}

export function trail(config, state) {
  return changeEnvs(config.environments).map((env) => state.changes[env.id]).filter(Boolean);
}
