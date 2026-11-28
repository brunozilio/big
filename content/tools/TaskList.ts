export const spec = {
  name: "TaskList",
  input: {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  description: `List the session's tasks (shared by the team, with owners and what blocks what) and the background agents of this run.`,
  profile: { effects: ["read"], risk: "low", reversible: true, untrusted: false, sensitive: false },
};

export function effects(_input: unknown) {
  return [];
}

export function summary(_input: unknown) {
  return "";
}

function planned(ctx) {
  try {
    return ctx.tasks.list();
  } catch (_error) {
    return [];
  }
}

export function execute(_input: unknown, ctx) {
  return JSON.stringify({ background: ctx.background.list(), planning: planned(ctx) });
}
