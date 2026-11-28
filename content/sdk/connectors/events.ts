export type Cadence = "poll" | "push";

export type EventSpec = {
  label: string;
  capability: string;
  properties: Record<string, unknown>;
  required?: string[];
  filters?: string[];
  aliases?: string[];
  source?: string;
  cadence?: Cadence;
  backfill?: string;
};

export const TEXT = { type: "string" };
export const MAYBE_TEXT = { type: ["string", "null"] };
export const NUMBER = { type: "number" };
export const FLAG = { type: "boolean" };

export function event(name: string, spec: EventSpec) {
  const { properties, required, ...rest } = spec;
  return {
    name,
    filters: [],
    aliases: [],
    cadence: "poll",
    backfill: "none",
    ...rest,
    schema: { type: "object", properties, required: required ?? [] },
  };
}
