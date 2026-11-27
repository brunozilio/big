import { changeEnvs } from "./rules.js";
import { shifted } from "./tasks.js";
import { fill } from "./gates.js";
import { projectOf } from "sdk/bindings.js";
import { changeSettings, windowOf } from "./change.js";

function described(state, config) {
  const labels = config.labels || {};
  const notes = state.members.map((member) => `- ${keyOf(member.issue) || member.id} ${member.title || ""}`.trim());
  const windows = changeEnvs(config.environments)
    .filter((env) => state.windows[env.id])
    .map((env) => `- ${env.label}: ${state.windows[env.id].replace("T", " ")}${state.changes[env.id] ? ` (${state.changes[env.id].key})` : ""}`);
  const tasks = state.tasks.map((task) => `- ${task.start || "--:--"}–${task.end || "--:--"} ${task.title}${task.owner ? ` · ${task.owner}` : ""}`);
  return [
    `## ${labels.notes || "Release notes"}`, ...notes, "",
    `## ${labels.windows || "Windows"}`, ...windows, "",
    `## ${labels.tasks || "Tasks"}`, ...tasks,
  ].join("\n");
}

function changePayload(state, config, envId) {
  const tasks = shifted(state, config, envId);
  const env = changeEnvs(config.environments).find((one) => one.id === envId) || { id: envId };
  return {
    title: state.title,
    ...windowOf(state, config, envId, tasks),
    settings: changeSettings(state.application, env).settings,
    tasks,
    notes: state.members.map((member) => ({ item: member.id, issue: member.issue || null, title: member.title || "" })),
    attachments: state.tasks.flatMap((task) => task.files || []),
    versions: Object.entries(state.versions[envId] || {}).map(([service, version]) => ({ service, rollback: version })),
    description: described(state, config),
  };
}

export async function openCard(ctx, state, config) {
  const card = config.card;
  if (!card || state.card || state.members.length === 0) return;
  const tracker = projectOf(ctx.item.application || state.application, "tracker");
  const project = card.project || tracker.project;
  if (!project) return;
  const options = cardOptions(tracker);
  const made = await ctx.call("tracker", "create_issue", { title: state.title, type: card.type, project, description: described(state, config) }, options);
  if (made && made.ref) state.card = { ref: made.ref, url: made.url || null, ...(tracker.connection ? { connection: tracker.connection } : {}) };
}

function cardOptions(card) {
  return card && card.connection ? { connection: card.connection } : { optional: true };
}

function keyOf(issue) {
  return issue && typeof issue === "object" ? issue.key : issue;
}

export function connectionOf(state, env) {
  const { connection } = changeSettings(state.application, env);
  return connection ? { connection } : {};
}

export async function publish(ctx, state, config) {
  state.revision = (state.revision || 0) + 1;
  await openCard(ctx, state, config);
  for (const env of changeEnvs(config.environments)) {
    const change = state.changes[env.id];
    if (!change || change.status === "done" || change.status === "cancelled") continue;
    await ctx.call("changes", "update", { change: change.ref, ...changePayload(state, config, env.id) }, connectionOf(state, env));
  }
  if (!state.card) return;
  await ctx.call("tracker", "update_issue", { issue: state.card.ref, title: state.title, description: described(state, config) }, cardOptions(state.card));
  for (const member of state.members) {
    if (!member.issue || state.cardLinks.includes(member.issue)) continue;
    await ctx.call("tracker", "link_issues", { issue: state.card.ref, to: member.issue, kind: (config.card && config.card.link) || undefined }, cardOptions(state.card));
    state.cardLinks.push(member.issue);
  }
}

export function changeArgs(state, config, envId) {
  return changePayload(state, config, envId);
}

export { fill };
