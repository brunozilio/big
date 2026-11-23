import { step, testCases } from "sdk";
import { agentOptions, defined, effects, inLanguage, issueOf, object, planOf, waitOptions } from "./context.js";

const KINDS = { api: "API", ui: "Tela" };

const APPROVE = "approve_cases";

const CHANGES = "change_cases";

const KEYWORDS = /^(dado|quando|então|entao|e|mas|given|when|then|and|but)\s+/i;

function clauses(found) {
  const list = Array.isArray(found) ? found : found ? [found] : [];
  return list.map((line) => String(line ?? "").trim().replace(KEYWORDS, "")).filter((line) => line.length > 0);
}

function fromSteps(entry) {
  const steps = (Array.isArray(entry.steps) ? entry.steps : []).map((one) => object(one)).filter((one) => one.action);
  return {
    given: clauses(entry.precondition),
    when: steps.map((one) => (one.data ? `${one.action} (${one.data})` : String(one.action))),
    then: steps.map((one) => String(one.expected ?? "")).filter((line) => line.length > 0),
  };
}

export function scenario(entry) {
  const found = object(entry);
  const source = found.given || found.when || found.then ? found : object(found.scenario);
  if (source.given || source.when || source.then) return { given: clauses(source.given), when: clauses(source.when), then: clauses(source.then) };
  return fromSteps(found);
}

export function bdd(entry) {
  const found = object(entry);
  return defined({
    title: String(found.title),
    kind: found.kind === "ui" ? "ui" : "api",
    objective: found.objective ? String(found.objective) : undefined,
    ...scenario(found),
    ref: found.ref ?? undefined,
    url: found.url ?? undefined,
  });
}

function written(found) {
  return (Array.isArray(found) ? found : [])
    .map((entry) => object(entry))
    .filter((entry) => entry.title)
    .map(bdd);
}

function gherkin(keyword, lines) {
  return lines.map((line, index) => `${index === 0 ? keyword : "E"} ${line}`).join("\n");
}

function fenced(text) {
  const found = String(text ?? "").match(/```(?:json)?\s*([\s\S]*?)```/);
  if (!found) return null;
  try {
    return JSON.parse(found[1]);
  } catch {
    return null;
  }
}

export function casesOf(result) {
  const output = object(result.output);
  if (Array.isArray(output.cases)) return written(output.cases);
  const parsed = fenced(result.text);
  return written(Array.isArray(parsed) ? parsed : object(parsed).cases);
}

function described(entry) {
  const one = bdd(entry);
  const head = `### ${one.ref ? `${one.ref} · ` : ""}${one.title} (${KINDS[one.kind] ?? one.kind})`;
  const parts = [head];
  if (one.objective) parts.push(one.objective);
  const lines = [gherkin("Dado", one.given), gherkin("Quando", one.when), gherkin("Então", one.then)].filter((text) => text.length > 0);
  parts.push(["```gherkin", `Cenário: ${one.title}`, ...lines.join("\n").split("\n").map((line) => `  ${line}`), "```"].join("\n"));
  return parts.join("\n\n");
}

export function listing(cases) {
  if (!Array.isArray(cases) || cases.length === 0) return "Nenhum caso de teste.";
  return cases.map(described).join("\n\n");
}

function asked(ctx, opts, issue, feedback) {
  if (opts.prompt) return opts.prompt;
  const plan = planOf(ctx, opts);
  const parts = [`Escreva os casos de teste E2E, de API e de tela, para o item: ${plan.title}`];
  if (issue) parts.push(`Issue: ${issue}`);
  if (plan.story) parts.push(plan.story);
  if (plan.text && plan.text !== plan.story) parts.push(`## Plano\n\n${plan.text}`);
  if (feedback) parts.push(`## Mudanças pedidas\n\n${feedback}`);
  return inLanguage(parts.join("\n\n"));
}

async function drafted(ctx, opts, issue, session, feedback) {
  const input = defined({ prompt: asked(ctx, opts, issue, feedback), session });
  const worktree = opts.worktree ?? ctx.item.branch ?? undefined;
  const result = await ctx.agent(opts.agent ?? "tester", input, agentOptions(opts, { mode: opts.mode ?? "plan", worktree }));
  return { session: result.session, cases: casesOf(result) };
}

async function judged(ctx, opts, draft) {
  if (opts.approve === "auto") return { approved: true };
  const view = [testCases(draft.cases, defined({ session: draft.session, approve: APPROVE, changes: CHANGES }))];
  for (;;) {
    const answer = await ctx.waitHuman(view, waitOptions(opts));
    if (answer.action === APPROVE) return { approved: true, by: answer.by ?? null };
    const changes = String(object(answer.values).changes ?? "").trim();
    if (changes) return { approved: false, changes };
  }
}

function chosen(ctx, field) {
  return object(object(ctx.item.application)[field]);
}

function testsOptions(ctx, opts) {
  const tests = chosen(ctx, "tests");
  return { ...opts, project: opts.project ?? tests.project, connection: opts.connection ?? tests.connection };
}

async function trackerId(ctx, opts, issue) {
  if (!issue || opts.link === false) return undefined;
  try {
    const found = await ctx.call("tracker", "issue", { issue }, effects({ ...opts, connection: opts.tracker ?? chosen(ctx, "tracker").connection }));
    return object(found).id ?? undefined;
  } catch {
    return undefined;
  }
}

async function created(ctx, given, cases, issue) {
  const opts = testsOptions(ctx, given);
  const issueId = await trackerId(ctx, opts, issue);
  const labels = Array.isArray(opts.labels) ? opts.labels : ["e2e"];
  const made = [];
  for (const entry of cases) {
    const precondition = gherkin("Dado", entry.given);
    const args = defined({
      project: opts.project,
      title: entry.title,
      objective: entry.objective,
      precondition: precondition || undefined,
      labels: [...labels, entry.kind === "ui" ? "tela" : "api"],
      steps: [{ action: gherkin("Quando", entry.when), expected: gherkin("Então", entry.then) }],
      issue: issue ?? undefined,
      issue_id: issueId,
    });
    const found = await ctx.call("tests", "create_case", args, effects(opts));
    made.push({ ...entry, ref: found.ref, url: found.url ?? null });
  }
  const title = issue ? `${issue} · ${planOf(ctx, opts).title}` : planOf(ctx, opts).title;
  const cycle = await ctx.call("tests", "create_cycle", defined({ project: opts.project, title, issue: issue ?? undefined, issue_id: issueId }), effects(opts));
  return { cases: made, cycle: cycle.ref, cycleUrl: cycle.url ?? null };
}

export default step({
  name: "cases",
  label: "Casos de teste",
  icon: "hNoteEdit",
  needs: "tests",
  doing: "Escrevendo casos de teste",
  view: (s) => (Array.isArray(s.cases) ? [testCases(s.cases.map(bdd), defined({ session: s.session }))] : []),
  async run(ctx, opts) {
    const issue = issueOf(ctx, opts);
    let draft = await drafted(ctx, opts, issue, undefined, undefined);
    for (;;) {
      if (draft.cases.length === 0) return ctx.fail({ session: draft.session, error: "o testador não escreveu nenhum caso de teste" });
      const verdict = await judged(ctx, opts, draft);
      if (verdict.approved) {
        const made = await created(ctx, opts, draft.cases, issue);
        return ctx.pass(defined({ session: draft.session, issue: issue ?? undefined, by: verdict.by, ...made }));
      }
      draft = await drafted(ctx, opts, issue, draft.session, verdict.changes);
    }
  },
});
