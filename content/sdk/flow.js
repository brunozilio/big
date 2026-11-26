function plain(value) {
  return value && typeof value === "object" && !Array.isArray(value) ? value : {};
}

const same = (left, right) => typeof left === "string" && typeof right === "string" && left.toLowerCase() === right.toLowerCase();

export function slugOf(text, most = 40) {
  const plainText = String(text ?? "")
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return plainText.slice(0, most).replace(/-+$/g, "");
}

function chosen(spec, type) {
  const entry = Object.entries(plain(spec.byType)).find(([name]) => same(name, type));
  return { ...spec, ...plain(entry ? entry[1] : null) };
}

export function filled(template, values) {
  const text = String(template).replace(/\{(\w+)\}/g, (_, name) => values[name] ?? "");
  return text.replace(/\/{2,}/g, "/").replace(/-{2,}/g, "-").replace(/[-/]+$/g, "").replace(/\/-/g, "/");
}

export function flow(spec) {
  const declared = plain(spec);
  return function branch(input) {
    const given = plain(input);
    const item = plain(given.item);
    const type = typeof item.type === "string" ? item.type : "";
    const rule = chosen(declared, type);
    const values = {
      issue: item.issue ? String(item.issue) : "",
      type: slugOf(type),
      slug: slugOf(item.title ?? item.text ?? ""),
      id: String(item.id ?? "").replace(/[^A-Za-z0-9]/g, "").slice(-6).toLowerCase(),
    };
    const found = {};
    if (typeof rule.branch === "string" && rule.branch !== "") found.head = filled(rule.branch, values);
    if (typeof rule.base === "string" && rule.base !== "") found.base = rule.base;
    if (typeof rule.method === "string" && rule.method !== "") found.method = rule.method;
    if (Array.isArray(rule.promote)) found.promote = rule.promote.map(plain).filter((step) => step.from && step.into);
    return Object.keys(found).length > 0 ? found : null;
  };
}
