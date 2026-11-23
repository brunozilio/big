export const FINISHED = ["passed", "failed", "cancelled"];

const LIMIT = 8000;

export function defined(fields) {
  const chosen = {};
  for (const [key, value] of Object.entries(fields)) {
    if (value !== undefined && value !== null) chosen[key] = value;
  }
  return chosen;
}

export function effects(opts) {
  return defined({ connection: opts.connection, timeout: opts.timeout, retry: opts.retry });
}

export function agentOptions(opts, extra) {
  return defined({
    model: opts.model,
    mode: opts.mode,
    askTimeout: opts.askTimeout,
    timeout: opts.timeout,
    retry: opts.retry,
    ...extra,
  });
}

export function tail(text, limit) {
  const value = String(text ?? "");
  const most = limit ?? LIMIT;
  return value.length > most ? value.slice(value.length - most) : value;
}

export function object(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value : {};
}

export function source(ctx, opts, kind) {
  const ids = object(opts.sources);
  return object(ctx.output(ids[kind] ?? kind));
}

export function planOf(ctx, opts) {
  const found = source(ctx, opts, "plan");
  return {
    title: opts.title ?? found.title ?? ctx.item.title ?? "",
    text: opts.plan ?? found.plan ?? "",
  };
}

export function issueOf(ctx, opts) {
  return opts.issue ?? source(ctx, opts, "ticket").issue ?? ctx.item.issue ?? null;
}

export function branchOf(ctx, opts) {
  return opts.branch ?? source(ctx, opts, "develop").branch ?? ctx.item.branch ?? null;
}

export function pullRequestOf(ctx, opts) {
  const found = source(ctx, opts, "pr");
  if (typeof opts.pr === "string") return { ...found, pr: opts.pr };
  return found.pr ? found : null;
}

export function waitOptions(opts) {
  return defined({ timeout: opts.waitTimeout });
}

function comment(entry) {
  const where = entry.path ? `${entry.path}${entry.line ? `:${entry.line}` : ""}: ` : "";
  return `- ${where}${entry.body ?? ""}`;
}

export function feedbackText(from) {
  if (!from) return "";
  const data = object(from.data);
  const parts = [`A etapa ${from.step} devolveu o trabalho.`];
  if (data.summary) parts.push(String(data.summary));
  if (data.error) parts.push(String(data.error));
  if (data.body) parts.push(String(data.body));
  if (Array.isArray(data.comments) && data.comments.length > 0) parts.push(data.comments.map(comment).join("\n"));
  if (data.output) parts.push(`\`\`\`\n${tail(data.output)}\n\`\`\``);
  return parts.join("\n\n");
}

export async function findRun(ctx, opts, wanted) {
  const args = defined({ ref: wanted.ref, pipeline: opts.pipeline, limit: 10 });
  for (;;) {
    const runs = await ctx.call("pipelines", "runs", args, effects(opts));
    const fresh = (Array.isArray(runs) ? runs : []).find((run) => !wanted.seen.includes(run.ref));
    if (fresh) return fresh;
    await ctx.sleep(opts.every ?? 30000);
  }
}

export async function settle(ctx, opts, run, until, events) {
  const target = defined({ capability: "pipelines", action: "run", args: { run }, connection: opts.connection });
  for (;;) {
    const waits = events.map((kind) => ctx.event(kind, { run }));
    const polled = ctx.poll(target, opts.every ?? 30000, defined({ until: { state: until }, timeout: opts.timeout }));
    const raced = await ctx.race([...waits, polled]);
    const current = raced.kind === "event" ? await ctx.call("pipelines", "run", { run }, effects(opts)) : raced.value;
    if (until.includes(current.state)) return current;
  }
}

export async function failedLogs(ctx, opts, run) {
  const failed = (Array.isArray(run.jobs) ? run.jobs : []).filter((job) => job.state === "failed");
  const texts = [];
  for (const job of failed) {
    const log = await ctx.call("pipelines", "job_log", { run: run.ref, job: job.id }, effects(opts));
    texts.push(`## ${job.name}\n${tail(object(log).text, 4000)}`);
  }
  return texts.join("\n\n");
}
