export const FINISHED = ["passed", "failed", "cancelled", "skipped"];
export const SUCCEEDED = ["passed", "skipped"];

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
    cloud: opts.cloud,
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

function sections(body) {
  const found = {};
  let name = "";
  for (const line of body.split("\n")) {
    const heading = /^##\s+(.+?)\s*$/.exec(line);
    if (heading) {
      name = heading[1].toLowerCase();
      found[name] = [];
    } else if (name) {
      found[name].push(line);
    }
  }
  return Object.fromEntries(Object.entries(found).map(([key, lines]) => [key, lines.join("\n").trim()]));
}

function subtasksOf(text) {
  return String(text ?? "")
    .split(/^###\s+/m)
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      const [first, ...rest] = part.split("\n");
      return { title: first.trim(), details: rest.join("\n").trim() };
    });
}

export function parsePlan(text) {
  const lines = String(text ?? "").split("\n");
  const at = lines.findIndex((line) => /^# \S/.test(line));
  if (at < 0) return { title: "", story: "", subtasks: [], text: String(text ?? "").trim() };
  const title = lines[at].slice(2).trim();
  const found = sections(lines.slice(at + 1).join("\n"));
  const description = found["descrição"] ?? found["descricao"] ?? "";
  const criteria = found["critérios de aceite"] ?? found["criterios de aceite"] ?? "";
  const story = [description, criteria ? `## Critérios de aceite\n${criteria}` : ""].filter(Boolean).join("\n\n");
  return { title, story, subtasks: subtasksOf(found["subtarefas"]), text: lines.slice(at).join("\n").trim() };
}

export function planOf(ctx, opts) {
  const found = source(ctx, opts, "plan");
  return {
    title: opts.title ?? found.title ?? ctx.item.title ?? "",
    text: opts.plan ?? found.plan ?? "",
    story: found.story ?? "",
    subtasks: Array.isArray(found.subtasks) ? found.subtasks : [],
  };
}

export function summaryLine(text) {
  const found = String(text ?? "").match(/^\**Resumo:?\**:?\s*(.+)$/im);
  if (found) return found[1].trim();
  const first = String(text ?? "").split(/\n\s*\n/).map((part) => part.trim()).find((part) => part && !part.startsWith("#"));
  return first ? first.split("\n")[0].slice(0, 280) : "";
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

export function pullRequestsOf(ctx, opts) {
  if (typeof opts.pr === "string") return [{ ...source(ctx, opts, "pr"), prs: undefined, pr: opts.pr }];
  const found = source(ctx, opts, "pr");
  if (Array.isArray(found.prs) && found.prs.length > 0) return found.prs.filter((entry) => entry && entry.pr);
  return found.pr ? [found] : [];
}

function folderName(directory) {
  const parts = String(directory ?? "").split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? "";
}

export function nameOf(repository) {
  const code = object(object(repository).code);
  return code.repo || folderName(object(repository).directory);
}

export function repositoriesOf(ctx) {
  const application = object(ctx.item.application);
  const listed = Array.isArray(application.repositories) ? application.repositories : [];
  return listed
    .filter((repository) => repository && repository.id)
    .map((repository) => ({ ...repository, name: nameOf(repository), code: repository.code ?? null }));
}

function touched(changes, target) {
  const found = changes.find((entry) => entry.id === target.id);
  return !found || found.changed !== false;
}

export function codeTargets(ctx, opts) {
  const repositories = repositoriesOf(ctx);
  if (repositories.length === 0) return [{ id: "", name: "", code: null }];
  const coded = repositories.filter((repository) => repository.code && repository.code.repo);
  if (coded.length === 0) return repositories.length === 1 ? [{ ...repositories[0], code: null }] : [];
  const changes = source(ctx, opts, "develop").repositories;
  if (!Array.isArray(changes)) return coded;
  const changed = coded.filter((target) => touched(changes, target));
  return changed.length > 0 ? changed : coded;
}

export function codeOptions(opts, target) {
  const code = object(object(target).code);
  return { ...opts, connection: opts.connection ?? code.connection };
}

export function pipelineOptions(opts, target) {
  const code = object(object(target).code);
  return { ...opts, connection: opts.connection ?? (code.pipelines ? code.connection : undefined) };
}

export function repoOf(target) {
  return object(object(target).code).repo ?? undefined;
}

export function targetFields(target) {
  return defined({ repository: target.id || target.repository, name: target.name, code: target.code });
}

export async function eachOf(ctx, targets, work) {
  if (targets.length === 1) return [await work(ctx, targets[0], 0)];
  return ctx.all(targets.map((target, index) => (branch) => work(branch, target, index)));
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
  const args = defined({ ref: wanted.ref, repo: wanted.repo, pipeline: opts.pipeline, limit: 10 });
  const patience = opts.patience ?? 10;
  for (let tries = 1; ; tries += 1) {
    const runs = await ctx.call("pipelines", "runs", args, effects(opts));
    const fresh = (Array.isArray(runs) ? runs : []).find((run) => !wanted.seen.includes(run.ref));
    if (fresh) return fresh;
    if (wanted.optional && tries >= patience) return null;
    if (wanted.optional && tries === 2) {
      const ever = await ctx.call("pipelines", "runs", defined({ repo: wanted.repo, pipeline: opts.pipeline, limit: 1 }), effects(opts));
      if (!Array.isArray(ever) || ever.length === 0) return null;
    }
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
