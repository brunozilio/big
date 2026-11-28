const unattended = {
  flagged: (ctx) => `${ctx.name} was blocked. ${ctx.reason.why} Find a safer way to do this or ask the user.`,
  hard: (ctx) =>
    `${ctx.name} was blocked because ${ctx.reason.why}. The safety floor has a person approve that every time, and this run has no one to ask, so it was refused. Find a safer way to do this or ask the user.`,
  declared: (ctx) => `${ctx.name} ${ctx.reason.why}, so a person approves it every time; it does not run unattended.`,
  flow: (ctx) => `${ctx.name} ${ctx.reason.why}, so a person approves it first, and this run has no one to ask.`,
  two: (ctx) =>
    `${ctx.name} ${ctx.reason.why}, so a person approves it first, and this run has no one to ask. Leave out the untrusted content or the sensitive data, or ask the user.`,
};

const inChat = (ctx) => ctx.surface === "chat";

const conduct = {
  declined: (ctx) => {
    if (!inChat(ctx)) return `${ctx.name} was refused by the person asked.`;
    if (ctx.note) return `The user did not allow ${ctx.name} here and said what to do instead: ${ctx.note}`;
    return `The user did not allow ${ctx.name} here. Do not retry it; ask them or take another approach.`;
  },
  expired: (ctx) =>
    inChat(ctx)
      ? `Not run: the run was stopped while waiting for the user's answer, so ${ctx.name} did not run and nothing was changed.`
      : `${ctx.name} was waiting for a person, and the item stopped first.`,
  blocked_by_policy: (ctx) => {
    const declared = ctx.reason && unattended[ctx.reason.kind];
    if (declared) return declared(ctx);
    return ctx.why || `${ctx.name} is blocked by policy.`;
  },
  unavailable: (ctx) => {
    const declared = ctx.reason && unattended[ctx.reason.kind];
    if (declared) return declared(ctx);
    if (inChat(ctx)) return `${ctx.name} requires permission; dontAsk denies unapproved actions.`;
    return `${ctx.name} asks a person first, and ${ctx.surface} cannot ask anyone yet.`;
  },
};

export default function outcome(ctx) {
  const told = conduct[ctx.outcome];
  return told ? told(ctx) : `${ctx.name} did not run.`;
}
