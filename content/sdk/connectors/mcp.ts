const ENVELOPES = ["result", "results", "data", "content", "value", "structuredContent"];

const isObject = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);

export function payload(value: unknown): unknown {
  if (typeof value === "string") {
    try {
      const parsed = JSON.parse(value.trim());
      if (Array.isArray(parsed) || isObject(parsed)) return payload(parsed);
    } catch (_) {
      return value;
    }
    return value;
  }
  if (isObject(value)) {
    const keys = Object.keys(value);
    if (keys.length !== 1) return value;
    const inner = value[keys[0]];
    if (Array.isArray(inner) || (ENVELOPES.includes(keys[0]) && inner !== null && inner !== undefined)) return payload(inner);
    return value;
  }
  return value;
}

export function items(value: unknown, field: string): any[] {
  const found = payload(value);
  if (Array.isArray(found)) return found;
  if (isObject(found)) {
    const inner = found[field];
    if (Array.isArray(inner)) return inner;
    if (typeof inner === "string") return items(inner, field);
  }
  return [];
}
