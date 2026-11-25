import { bindingOf } from "sdk/bindings.js";

const LOCAL = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$/;
const ZONE = /^(Z|[+-]\d{2}:\d{2})$/;
const CLOCK = /^\d{2}:\d{2}$/;

const pad = (number) => String(number).padStart(2, "0");
const minutesOf = (clock) => {
  const [hours, minutes] = String(clock).split(":").map(Number);
  return hours * 60 + minutes;
};

export function zoneOf(value, fallback) {
  const given = String(value || "").trim();
  if (ZONE.test(given)) return given;
  return ZONE.test(String(fallback || "")) ? fallback : "Z";
}

export function isoAt(local, zone, plus) {
  const parts = LOCAL.exec(String(local || ""));
  if (!parts) return null;
  const [, year, month, day, hours, minutes, seconds] = parts.map((part) => Number(part || 0));
  const at = new Date(Date.UTC(year, month - 1, day, hours, minutes + (plus || 0), seconds));
  const date = `${at.getUTCFullYear()}-${pad(at.getUTCMonth() + 1)}-${pad(at.getUTCDate())}`;
  const time = `${pad(at.getUTCHours())}:${pad(at.getUTCMinutes())}:${pad(at.getUTCSeconds())}`;
  return `${date}T${time}${zone}`;
}

export function spanOf(window, tasks) {
  const clock = String(window || "").split("T")[1] || "";
  if (!CLOCK.test(clock.slice(0, 5))) return 0;
  const from = minutesOf(clock.slice(0, 5));
  const ends = (tasks || []).map((task) => task.end).filter((end) => CLOCK.test(String(end || "")));
  return ends.reduce((longest, end) => Math.max(longest, (((minutesOf(end) - from) % 1440) + 1440) % 1440), 0);
}

export function windowOf(state, config, envId, tasks) {
  const local = state.windows[envId];
  const zone = zoneOf((state.zones || {})[envId], config.zone);
  return { start: isoAt(local, zone, 0), end: isoAt(local, zone, spanOf(local, tasks)) };
}

function names(env) {
  const gate = env.gate || {};
  return [gate.environment, env.id, env.short, env.label].filter(Boolean).map((name) => String(name).toLowerCase());
}

function matches(profile, env) {
  const wanted = names(env);
  return Object.values(profile || {}).some((choice) => [choice && choice.value, choice && choice.label].some((one) => one && wanted.includes(String(one).toLowerCase())));
}

const valuesOf = (fields) => Object.fromEntries(Object.entries(fields || {}).map(([key, choice]) => [key, choice && typeof choice === "object" ? choice.value : choice]));

function changesBound(application) {
  const bound = bindingOf(application, "changes") || (application && application.bindings && application.bindings.changes) || null;
  return bound && bound.connection ? bound : null;
}

export function changeConnection(application) {
  const bound = changesBound(application);
  return bound ? { connection: bound.connection } : {};
}

export function changeSettings(application, env) {
  const bound = changesBound(application);
  if (!bound || !env || !env.id) return { connection: bound ? bound.connection : null, settings: {} };
  const profiles = Array.isArray(bound.profiles) ? bound.profiles : [];
  const profile = profiles.find((one) => matches(one, env));
  if (profiles.length > 0 && !profile) throw new Error(`the application has no change profile for the environment ${env.label || env.id}`);
  return { connection: bound.connection, settings: { ...valuesOf(bound.fields), ...valuesOf(profile) } };
}

export async function ownersOf(ctx, using) {
  let found = null;
  try {
    found = await ctx.call("changes", "options", { field: "owner" }, { ...(using || {}), optional: true });
  } catch {
    found = null;
  }
  return Array.isArray(found) ? found.filter((one) => one && one.value).map((one) => ({ value: String(one.value), label: String(one.label || one.value) })) : [];
}
