const LEGACY = ["value", "repo", "project", "account"];

function plain(value) {
  return value && typeof value === "object" && !Array.isArray(value) ? value : {};
}

export function bindingOf(holder, capability) {
  const found = plain(holder);
  const bound = plain(plain(found.bindings)[capability]);
  if (bound.connection && bound.value) return bound;
  const legacy = plain(found[capability]);
  const value = LEGACY.map((name) => legacy[name]).find(Boolean);
  return legacy.connection && value ? { ...legacy, value } : null;
}

export function offers(bound, capability) {
  const listed = Array.isArray(plain(bound).capabilities) ? bound.capabilities : [];
  return listed.includes(capability) || plain(bound)[capability] === true;
}

export function codeOf(repository) {
  const bound = bindingOf(repository, "code");
  if (!bound) return null;
  return { connection: bound.connection, repo: bound.value, pipelines: offers(bound, "pipelines") };
}

export function projectOf(holder, capability) {
  const bound = bindingOf(holder, capability);
  return bound ? { connection: bound.connection, project: bound.value } : {};
}
