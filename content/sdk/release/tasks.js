import { changeEnvs } from "./rules.js";

const fill = (text, values) => String(text || "").replace(/\{(\w+)\}/g, (_, name) => (values[name] === undefined ? "" : String(values[name])));

export function makeTask(state, kind, fields) {
  state.taskSeq = (state.taskSeq || 0) + 1;
  return { id: `t${state.taskSeq}`, kind, title: "", detail: "", owner: null, link: null, vars: {}, start: "", end: "", files: [], ...fields };
}

export function serviceTasks(state, config, services) {
  const templates = config.tasks || {};
  const deploy = templates.deploy || {};
  const rollback = templates.rollback || {};
  const made = { deploys: [], rollbacks: [] };
  for (const service of services) {
    const values = { service: service.name, version: service.version, previous: service.previous || "", repo: service.repo, pipeline: service.pipeline || "" };
    made.deploys.push(makeTask(state, "deploy", { title: fill(deploy.title || "Deploy {service} {version}", values), owner: deploy.owner || null, link: fill(deploy.link, values) || null, service: service.name, vars: { version: service.version } }));
    made.rollbacks.push(makeTask(state, "rollback", { title: fill(rollback.title || "Rollback {service}", values), owner: rollback.owner || null, link: fill(rollback.link, values) || null, service: service.name, vars: {} }));
  }
  return made;
}

export function defaultTasks(state, config, services) {
  const made = serviceTasks(state, config, services);
  const extras = (config.tasks && config.tasks.kinds) || [];
  const defaults = extras.filter((kind) => kind.byDefault).map((kind) => makeTask(state, kind.kind, { title: kind.title || kind.label, detail: kind.detail || "", owner: kind.owner || null, template: kind.id }));
  return [...made.deploys, ...defaults, ...made.rollbacks];
}

export function addTask(state, config, templateId) {
  const template = ((config.tasks && config.tasks.kinds) || []).find((kind) => kind.id === templateId);
  if (!template) return null;
  return makeTask(state, template.kind, { title: template.title || template.label, detail: template.detail || "", owner: template.owner || null, link: template.link || null, template: template.id });
}

export function refreshServices(state, config, services) {
  const names = services.map((service) => service.name);
  const kept = state.tasks.filter((task) => !["deploy", "rollback"].includes(task.kind) || names.includes(task.service));
  const has = (kind, name) => kept.some((task) => task.kind === kind && task.service === name);
  const missing = services.filter((service) => !has("deploy", service.name) || !has("rollback", service.name));
  const made = serviceTasks(state, config, missing);
  const deploys = made.deploys.filter((task) => !has("deploy", task.service));
  const rollbacks = made.rollbacks.filter((task) => !has("rollback", task.service));
  state.tasks = [...deploys, ...kept, ...rollbacks];
}

const minutesOf = (clock) => {
  const [hours, minutes] = String(clock).split(":").map(Number);
  return hours * 60 + minutes;
};
const clockOf = (total) => {
  const minutes = ((total % 1440) + 1440) % 1440;
  return `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
};
const timeOf = (window) => (window ? String(window).split("T")[1] || null : null);

export function schedule(state, config) {
  const at = timeOf(state.windows[anchorOf(state, config)]);
  if (!at) return;
  const durations = (config.tasks && config.tasks.minutes) || {};
  let cursor = minutesOf(at);
  state.tasks = state.tasks.map((task) => {
    if (task.start && task.end) {
      cursor = minutesOf(task.end);
      return task;
    }
    const start = cursor;
    cursor += Number(durations[task.kind] || 15);
    return { ...task, start: clockOf(start), end: clockOf(cursor) };
  });
}

export function anchorOf(state, config) {
  const first = changeEnvs(config.environments)[0];
  return first ? first.id : null;
}

export function unshifted(state, config, envId, list) {
  const back = { ...state, tasks: list, windows: { ...state.windows } };
  const anchor = anchorOf(state, config);
  if (!envId || envId === anchor || !state.windows[envId] || !state.windows[anchor]) return list;
  back.windows[anchor] = state.windows[envId];
  back.windows[envId] = state.windows[anchor];
  return shifted(back, config, envId);
}

export function shifted(state, config, envId) {
  const anchor = timeOf(state.windows[anchorOf(state, config)]);
  const here = timeOf(state.windows[envId]);
  if (!anchor || !here) return state.tasks;
  const delta = minutesOf(here) - minutesOf(anchor);
  const move = (clock) => (/^\d{2}:\d{2}$/.test(clock) ? clockOf(minutesOf(clock) + delta) : clock);
  return state.tasks.map((task) => ({ ...task, start: move(task.start), end: move(task.end) }));
}

export function sanitize(list, config) {
  const kinds = new Set(["deploy", "rollback", ...(((config.tasks && config.tasks.kinds) || []).map((kind) => kind.kind))]);
  const attachable = new Set(((config.tasks && config.tasks.kinds) || []).filter((kind) => kind.attachable).map((kind) => kind.kind));
  return (Array.isArray(list) ? list : [])
    .filter((task) => task && kinds.has(task.kind) && typeof task.id === "string")
    .map((task) => ({
      ...task,
      title: String(task.title || ""),
      detail: String(task.detail || ""),
      start: String(task.start || ""),
      end: String(task.end || ""),
      files: attachable.has(task.kind) && Array.isArray(task.files) ? task.files : [],
    }));
}
