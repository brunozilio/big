import { type Model, capabilities, option, positive } from "sdk/providers/index.ts";

const EFFORTS = ["low", "medium", "high", "xhigh", "max"];
const FIELDS = ["output_config", "reasoning"];

const thinking = (adaptive: boolean, levels: string[]) => ({ reasoning: true, adaptive_thinking: adaptive, reasoning_levels: levels });

function declared(schema) {
  for (const field of FIELDS) {
    const listed = schema?.properties?.[field]?.properties?.effort?.enum;
    if (!Array.isArray(listed)) continue;
    const levels = listed.filter((level) => typeof level === "string");
    if (levels.length > 0) return thinking(field === "output_config", levels);
  }
  return undefined;
}

const leading = (text: string) => {
  const digits = /^\d+/.exec(text)?.[0];
  return digits === undefined ? undefined : Number.parseInt(digits, 10);
};

function known(model: string) {
  if (!model.startsWith("claude-")) return undefined;
  const rest = model.slice("claude-".length);
  const family = ["opus-", "sonnet-"].find((each) => rest.startsWith(each));
  if (family === undefined) return undefined;
  const [first, second] = rest.slice(family.length).split(".", 2);
  const major = leading(first ?? "");
  const minor = second === undefined ? 0 : leading(second);
  if (major === undefined || minor === undefined) return undefined;
  if (major === 4 && minor === 6) return thinking(true, ["low", "medium", "high", "max"]);
  if ((major === 4 && minor >= 7) || major >= 5) return thinking(true, [...EFFORTS]);
  return undefined;
}

function chosen(levels: string[], effort: string): string {
  const found = EFFORTS.indexOf(effort);
  const asked = found < 0 ? 1 : found;
  return (
    [...EFFORTS.slice(0, asked + 1)].reverse().find((level) => levels.includes(level)) ??
    levels.find((given) => EFFORTS.includes(given)) ??
    levels[0] ??
    ""
  );
}

export function pickEffort(model: Model | null, body, effort: string) {
  if (!model) return body;
  const bare = model.id.split(":").pop() ?? model.id;
  const listed = model.capabilities && Array.isArray(model.capabilities.reasoning_levels) && model.capabilities.reasoning_levels.length > 0 ? model.capabilities : undefined;
  const caps = listed ?? known(bare);
  if (!caps || bare === "auto") return body;
  const field = caps.adaptive_thinking ? "output_config" : "reasoning";
  body.additionalModelRequestFields = { [field]: { effort: chosen(caps.reasoning_levels as string[], effort) } };
  return body;
}

function modelOf(ctx, item): Model | null {
  const id = typeof item?.modelId === "string" ? item.modelId.trim() : "";
  if (id === "") return null;
  const name = typeof item.modelName === "string" ? item.modelName.trim() : "";
  const found = option(ctx, id, name === "" ? id : name);
  const limits = item.tokenLimits ?? {};
  found.window = positive(limits.maxInputTokens);
  const thinks = declared(item.additionalModelRequestFieldsSchema) ?? known(id) ?? {};
  found.capabilities = capabilities({ ...thinks, max_output_tokens: positive(limits.maxOutputTokens) });
  return found;
}

export function page(ctx, value) {
  if (!Array.isArray(value?.models)) return undefined;
  const models = value.models.map((item) => modelOf(ctx, item)).filter((found) => found !== null);
  const next = typeof value.nextToken === "string" && value.nextToken !== "" ? value.nextToken : null;
  return { models, next };
}
