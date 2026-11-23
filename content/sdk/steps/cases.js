import { step, actions, chat, form, testCases } from "sdk";
import { agentOptions, defined, effects, inLanguage, issueOf, object, planOf, waitOptions } from "./context.js";

const KINDS = { api: "API", ui: "Tela" };

const APPROVE = "approve_cases";

function steps(found) {
  return (Array.isArray(found) ? found : [])
    .map((entry) => object(entry))
    .filter((entry) => entry.action)
    .map((entry) => defined({ action: String(entry.action), data: entry.data ? String(entry.data) : undefined, expected: String(entry.expected ?? "") }));
}

function written(found) {
  return (Array.isArray(found) ? found : [])
    .map((entry) => object(entry))
    .filter((entry) => entry.title)
    .map((entry) =>
      defined({
        title: String(entry.title),
        kind: entry.kind === "ui" ? "ui" : "api",
        objective: entry.objective ? String(entry.objective) : undefined,
        precondition: entry.precondition ? String(entry.precondition) : undefined,
        steps: steps(entry.steps),
      }),
    );
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
  const head = `### ${entry.ref ? `${entry.ref} · ` : ""}${entry.title} (${KINDS[entry.kind] ?? entry.kind})`;
  const parts = [head];
  if (entry.objective) parts.push(entry.objective);
  if (entry.precondition) parts.push(`**Pré-condição:** ${entry.precondition}`);
  const lines = (entry.steps ?? []).map((one, index) => `${index + 1}. ${one.action}${one.data ? ` (${one.data})` : ""} → ${one.expected}`);
  if (lines.length > 0) parts.push(lines.join("\n"));
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
  const view = [
    testCases(draft.cases),
    chat(draft.session),
    form([{ id: "changes", label: "O que mudar nos casos", kind: "textarea" }]),
    actions([{ id: APPROVE, label: "Criar casos" }]),
  ];
  for (;;) {
    const answer = await ctx.waitHuman(view, waitOptions(opts));
    if (answer.action === APPROVE) return { approved: true, by: answer.by ?? null };
    const changes = String(object(answer.values).changes ?? "").trim();
    if (changes) return { approved: false, changes };
  }
}

async function trackerId(ctx, opts, issue) {
  if (!issue || opts.link === false) return undefined;
  try {
    const found = await ctx.call("tracker", "issue", { issue }, effects({ ...opts, connection: opts.tracker }));
    return object(found).id ?? undefined;
  } catch {
    return undefined;
  }
}

async function created(ctx, opts, cases, issue) {
  const issueId = await trackerId(ctx, opts, issue);
  const labels = Array.isArray(opts.labels) ? opts.labels : ["e2e"];
  const made = [];
  for (const entry of cases) {
    const args = defined({
      project: opts.project,
      title: entry.title,
      objective: entry.objective,
      precondition: entry.precondition,
      labels: [...labels, entry.kind === "ui" ? "tela" : "api"],
      steps: entry.steps,
      issue: issue ?? undefined,
      issue_id: issueId,
    });
    const found = await ctx.call("tests", "create_case", args, effects(opts));
    made.push({ ...entry, ref: found.ref, url: found.url ?? null });
  }
  const title = issue ? `${issue} · ${planOf(ctx, opts).title}` : planOf(ctx, opts).title;
  const cycle = await ctx.call("tests", "create_cycle", defined({ project: opts.project, title, issue_id: issueId }), effects(opts));
  return { cases: made, cycle: cycle.ref, cycleUrl: cycle.url ?? null };
}

export default step({
  name: "cases",
  label: "Casos de teste",
  icon: "hNoteEdit",
  needs: "tests",
  doing: "Escrevendo casos de teste",
  view: (s) => [...(Array.isArray(s.cases) ? [testCases(s.cases)] : []), ...(s.session ? [chat(s.session)] : [])],
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
