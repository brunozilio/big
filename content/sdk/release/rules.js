export const PASSED = "passed";
export const RUNNING = "running";
export const WAITING = "waiting_human";
export const PENDING = "pending";

export function environments(config) {
  const list = (config && config.environments) || [];
  if (list.length === 0) throw new Error("the promotion needs at least one environment");
  const ids = new Set();
  for (const env of list) {
    if (!env.id || ids.has(env.id)) throw new Error(`the environment id ${env.id} is missing or repeated`);
    ids.add(env.id);
    if (!env.gate || !["none", "merge", "change"].includes(env.gate.kind)) {
      throw new Error(`the environment ${env.id} needs a gate of kind none, merge or change`);
    }
  }
  return list;
}

export const after = (envs, id) => envs.slice(envs.findIndex((env) => env.id === id) + 1);
export const before = (envs, id) => envs.slice(0, Math.max(0, envs.findIndex((env) => env.id === id)));
export const changeEnvs = (envs) => envs.filter((env) => env.gate.kind === "change");

export function initialStages(envs) {
  const stages = {};
  envs.forEach((env, index) => {
    stages[env.id] = env.gate.kind === "none" && index === 0 ? PASSED : PENDING;
  });
  const first = envs.find((env) => stages[env.id] !== PASSED);
  if (first) stages[first.id] = WAITING;
  return stages;
}

export function sealing(envs) {
  return envs.find((env) => env.seals) || changeEnvs(envs)[0] || null;
}

export function accepting(envs, state) {
  const seal = sealing(envs);
  return !seal || !state.changes[seal.id];
}

export function visible(envs, state, env) {
  if (env.gate.kind !== "change") return true;
  const earlier = changeEnvs(envs).filter((other) => envs.indexOf(other) < envs.indexOf(env));
  return earlier.every((other) => Boolean(state.changes[other.id]));
}

export function canCreateChange(envs, state, env) {
  return env.gate.kind === "change" && !state.changes[env.id] && state.stages[env.id] === WAITING && Boolean(state.windows[env.id]);
}

export function canPromote(state, env) {
  return env.gate.kind === "merge" && state.stages[env.id] === WAITING;
}

export function canRollback(envs, state, env) {
  if (!env.rollback || env.gate.kind !== "merge" || state.stages[env.id] !== PASSED) return false;
  const later = after(envs, env.id);
  if (later.some((other) => state.changes[other.id])) return false;
  const next = later[0];
  return !next || ![PASSED, RUNNING].includes(state.stages[next.id]);
}

export function tasksLocked(envs, state) {
  const locking = envs.find((env) => env.locksTasks) || changeEnvs(envs).slice(-1)[0];
  if (locking && state.changes[locking.id]) return true;
  return changeEnvs(envs).some((env) => {
    const change = state.changes[env.id];
    return change && change.status !== "done";
  });
}

export function current(envs, state) {
  const shown = changeEnvs(envs).filter((env) => visible(envs, state, env));
  return shown.find((env) => !state.changes[env.id] || state.changes[env.id].status !== "done") || shown.slice(-1)[0] || null;
}

export function delivered(envs, state) {
  return envs.every((env) => state.stages[env.id] === PASSED);
}
