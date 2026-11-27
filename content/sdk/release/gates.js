import { PASSED, RUNNING, WAITING, after } from "./rules.js";
import { codeOf } from "sdk/bindings.js";

export const fill = (text, values) =>
  String(text || "").replace(/\{(\w+)\}/g, (_, name) => (values[name] === undefined ? "" : String(values[name])));

function repositoriesOf(member) {
  const named = Object.keys((member && member.git) || {});
  const sourced = ((member && member.sources) || []).map((source) => source.repository).filter(Boolean);
  return [...new Set([...named, ...sourced])];
}

function promotionsOf(state, repository) {
  for (const member of state.members) {
    const found = ((member && member.git) || {})[repository];
    if (found && Array.isArray(found.promote) && found.promote.length > 0) return found.promote;
  }
  return null;
}

export function gateFor(env, service) {
  const gate = env.gate || {};
  const at = gate.promotion;
  const steps = service && Array.isArray(service.promote) ? service.promote : null;
  if (gate.kind !== "merge" || !Number.isInteger(at) || !steps || !steps[at]) return gate;
  return { ...gate, ...steps[at] };
}

export function services(state, config, application) {
  const repositories = (application && application.repositories) || [];
  const wanted = new Set(state.members.flatMap(repositoriesOf));
  const chosen = repositories
    .filter((repository) => repository.readOnly !== true)
    .filter((repository) => wanted.size === 0 || wanted.has(repository.id))
    .map((repository) => ({ ...repository, code: codeOf(repository) }));
  return chosen
    .filter((repository) => repository.code && repository.code.repo)
    .map((repository) => {
      const given = ((config.services || []).find((service) => service.repository === repository.id || service.repo === repository.code.repo)) || {};
      const sha = state.merged[repository.id] || "";
      return {
        id: repository.id,
        name: given.name || repository.id,
        repo: repository.code.repo,
        connection: repository.code.connection,
        pipeline: given.pipeline || (config.rollbackVersion && config.rollbackVersion.pipeline) || "",
        version: given.version || (sha ? sha.slice(0, 7) : "next"),
        promote: promotionsOf(state, repository.id),
      };
    });
}

async function quietly(work) {
  try {
    return await work();
  } catch (error) {
    return { failed: error && error.message ? error.message : String(error) };
  }
}

export async function promote(ctx, state, config, env, application) {
  state.stages[env.id] = RUNNING;
  for (const service of services(state, config, application)) {
    const gate = gateFor(env, service);
    const options = { connection: service.connection };
    const title = fill(gate.title || "Promote {from} to {into}", gate);
    const pull = await quietly(() => ctx.call("code", "open_pull_request", { repo: service.repo, head: gate.from, base: gate.into, title }, options));
    if (pull.failed || !pull.ref) continue;
    const merged = await ctx.call("code", "merge", { pr: pull.ref, method: gate.method || "merge", delete_branch: false }, options);
    if (merged && merged.sha) state.merged[service.id] = merged.sha;
    state.merges[env.id] = { ...(state.merges[env.id] || {}), [service.id]: (merged && merged.sha) || null };
  }
  state.stages[env.id] = PASSED;
  const next = after(config.environments, env.id)[0];
  if (next && state.stages[next.id] !== PASSED) state.stages[next.id] = WAITING;
}

const DEFAULT_REVERT = "git fetch origin {branch} && git checkout -B {branch} origin/{branch} && git revert -m 1 --no-edit {sha} && git push origin {branch}";

export async function rollback(ctx, state, config, env, application) {
  state.stages[env.id] = RUNNING;
  state.reverting = env.id;
  const command = (env.rollback && env.rollback.command) || DEFAULT_REVERT;
  const listed = services(state, config, application);
  const shas = env.gate.kind === "merge" ? state.merges[env.id] || {} : state.memberShas || {};
  for (const [repository, sha] of Object.entries(shas)) {
    const gate = gateFor(env, listed.find((service) => service.id === repository));
    const branch = env.branch || gate.into || (env.lane === true ? promotedOf(listed.find((service) => service.id === repository)).from : "") || "";
    for (const one of [].concat(sha || [])) await ctx.exec(fill(command, { branch, sha: one }), { repository });
  }
  state.reverting = null;
  state.stages[env.id] = WAITING;
  for (const later of after(config.environments, env.id)) state.stages[later.id] = "pending";
}

function promotedOf(service) {
  const steps = service && Array.isArray(service.promote) ? service.promote : [];
  const first = steps[0];
  return first && first.into ? { into: first.into, from: first.from } : {};
}

export async function versions(ctx, config, env, list) {
  const finder = { ...(config.rollbackVersion || {}), ...((env && env.rollbackVersion) || {}) };
  const found = {};
  for (const service of list) {
    if (finder.kind === "cloud") {
      const last = await ctx.call("cloud", "last_deployment", { service: service.name, environment: env.id }, { optional: true });
      found[service.name] = (last && last.version) || null;
      continue;
    }
    const args = { repo: service.repo, pipeline: service.pipeline, ref: fill(finder.ref || "", { ...env, ...promotedOf(service) }), status: "success", limit: 1 };
    const runs = await quietly(() => ctx.call("pipelines", "runs", args, { connection: service.connection, optional: true }));
    const last = Array.isArray(runs) ? runs[0] : null;
    found[service.name] = last ? String(last.head || last.ref || "") : null;
  }
  return found;
}

export async function createChange(ctx, state, config, env, payload, using) {
  state.versions[env.id] = await versions(ctx, config, env, state.services);
  if (!using) {
    state.changes[env.id] = { ref: `local:${env.id}`, key: env.short || env.label || env.id, status: "awaiting_approval", url: null, connection: null, local: true };
    state.stages[env.id] = RUNNING;
    return;
  }
  const made = await ctx.call("changes", "create", { title: state.title, environment: env.id, ...payload }, using);
  state.changes[env.id] = { ref: made.ref, key: made.key, status: made.status, url: made.url || null, connection: (using && using.connection) || null };
  state.stages[env.id] = RUNNING;
}

export function settleChange(state, config, env, status) {
  const change = state.changes[env.id];
  if (!change) return false;
  if (change.status === status) return false;
  change.status = status;
  if (status === "done") {
    state.stages[env.id] = PASSED;
    const next = after(config.environments, env.id)[0];
    if (next && state.stages[next.id] !== PASSED) state.stages[next.id] = WAITING;
  }
  if (status === "cancelled" || status === "rejected") state.stages[env.id] = "failed";
  return true;
}
