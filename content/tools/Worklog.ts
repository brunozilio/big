const DAY_MS = 24 * 60 * 60 * 1000;

export const spec = {
  name: "Worklog",
  deferred: true,
  input: {
    additionalProperties: false,
    properties: {
      action: {
        description: "`items` lists the work items touched in the period or still open, each with what was done in the period (work_done) and what is left (next_steps); `item` shows one item in full; `cost` adds up what the period's runs cost, per item and step, with conversations apart.",
        enum: ["items", "item", "cost"],
        type: "string",
      },
      person: { description: "items, cost: whose work, by user id or e-mail; the person you work for if omitted. Reading someone else's needs the admin grant, or the team grant and a team in common.", type: "string" },
      days: { description: "the last this many days (1 if omitted; for `item`, its whole life).", type: "integer" },
      since: { description: "start of the period, an ISO date or date-time; overrides days.", type: "string" },
      until: { description: "end of the period, an ISO date or date-time (now if omitted).", type: "string" },
      item: { description: "item: the work item id.", type: "string" },
      limit: { description: "items: how many to return (20 if omitted, at most 100).", type: "integer" },
    },
    required: ["action"],
    type: "object",
  },
  description: (ctx) => `Read, never change, the work ${ctx.brand.NAME} keeps: work items, their steps, checkpoints of what was done in a period and what is left, and what the runs cost. It sees only what the person you work for may see. Use it for a standup ("what did I do yesterday, what is left"), a status of one item, or a cost report, instead of guessing or reading conversations.`,
  profile: { effects: ["read"], risk: "low", reversible: true, untrusted: true, sensitive: true },
};

type Input = {
  action: "items" | "item" | "cost";
  person?: string;
  days?: number;
  since?: string;
  until?: string;
  item?: string;
  limit?: number;
};

type Spend = { usd: number; unpriced_calls: number };
type Done = { kind: string; step?: string; state?: string; action?: string; status?: string; reason?: string; at_ms: number };
type Next = { kind: string; step?: string; reason?: string | null };
type Step = { id: string; state: string; attempt: number; started_ms?: number; finished_ms?: number };
type Item = {
  id: string;
  title: string;
  workflow: string;
  owner: string;
  group: string;
  status?: string;
  ended: boolean;
  updated_ms?: number;
  recap?: string;
  steps: Step[];
  checkpoint: { work_done: Done[]; next_steps: Next[] };
  cost: Spend;
};
type Cost = Spend & {
  person?: string;
  since_ms: number;
  until_ms: number;
  conversations: Spend;
  items: (Spend & { id: string; title: string; steps: (Spend & { step: string })[] })[];
};

export function offered(ctx) {
  return ctx.services?.worklog === true;
}

export function effects(_input: Input | null) {
  return [];
}

export function summary(input: Input | null) {
  if (!input) {
    return "";
  }
  return [input.action, input.item, input.person].filter(Boolean).join(" ");
}

function when(ms: number | undefined): string {
  return ms ? new Date(ms).toISOString().slice(0, 16).replace("T", " ") : "";
}

function moment(text: string | undefined): number | undefined {
  if (!text) {
    return undefined;
  }
  const at = Date.parse(text);
  if (Number.isNaN(at)) {
    throw new Error(`${text} is not a date`);
  }
  return at;
}

function period(input: Input) {
  const until = moment(input.until) ?? Date.now();
  const whole = input.action === "item" && !input.days && !input.since;
  const since = moment(input.since) ?? (whole ? 0 : until - Math.max(1, input.days ?? 1) * DAY_MS);
  return { since_ms: since, until_ms: until, person: input.person, limit: input.limit };
}

function usd(spend: Spend): string {
  const priced = `$${spend.usd.toFixed(2)}`;
  return spend.unpriced_calls > 0 ? `${priced} + ${spend.unpriced_calls} unpriced calls` : priced;
}

function done(entry: Done): string {
  const at = when(entry.at_ms);
  switch (entry.kind) {
    case "step":
      return `${at} step ${entry.step} ${entry.state}`;
    case "answered":
      return `${at} answered ${entry.step}: ${entry.action}`;
    case "status":
      return `${at} status ${entry.status}`;
    case "cancelled":
      return `${at} cancelled: ${entry.reason}`;
    default:
      return `${at} ${entry.kind}`;
  }
}

function next(entry: Next): string {
  if (entry.kind === "impediment") {
    return `blocked${entry.reason ? `: ${entry.reason}` : ""}`;
  }
  return `${entry.step} ${entry.kind.replace("_", " ")}`;
}

function shownItem(item: Item): string {
  const head = `${item.title} [${item.id}] ${item.workflow}, ${item.ended ? "ended" : item.group.replace("_", " ")}${item.status ? `, status ${item.status}` : ""}, owner ${item.owner}, last ${when(item.updated_ms)}, cost ${usd(item.cost)}`;
  const doneLines = item.checkpoint.work_done.map((entry) => `  done: ${done(entry)}`);
  const nextLines = item.checkpoint.next_steps.map((entry) => `  next: ${next(entry)}`);
  const recap = item.recap ? [`  recap: ${item.recap}`] : [];
  return [head, ...doneLines, ...nextLines, ...recap].join("\n");
}

function items(input: Input, ctx): string {
  const found: Item[] = ctx.worklog.items(period(input));
  return found.length === 0 ? "No work item was touched in the period and none is open." : found.map(shownItem).join("\n");
}

function item(input: Input, ctx): string {
  const found: Item = ctx.worklog.item({ ...period(input), item: input.item ?? "" });
  const steps = found.steps.map(
    (step) => `  step ${step.id} ${step.state}, attempt ${step.attempt}, started ${when(step.started_ms)}${step.finished_ms ? `, finished ${when(step.finished_ms)}` : ""}`,
  );
  return [shownItem(found), ...steps].join("\n");
}

function cost(input: Input, ctx): string {
  const found: Cost = ctx.worklog.cost(period(input));
  const whose = found.person ? ` for ${found.person}` : "";
  const head = `${usd(found)} from ${when(found.since_ms)} to ${when(found.until_ms)}${whose}; conversations ${usd(found.conversations)}`;
  const lines = found.items.map((entry) => {
    const steps = entry.steps.map((step) => `${step.step} ${usd(step)}`).join(", ");
    return `  ${entry.title} [${entry.id}] ${usd(entry)}: ${steps}`;
  });
  return [head, ...lines].join("\n");
}

export function execute(input: Input, ctx) {
  try {
    switch (input.action) {
      case "items":
        return items(input, ctx);
      case "item":
        return item(input, ctx);
      case "cost":
        return cost(input, ctx);
      default:
        return { content: `Unknown action ${input.action}.`, isError: true };
    }
  } catch (error) {
    return { content: String(error.message ?? error), isError: true };
  }
}
