import { step, markdown, testCases } from "sdk";
import { agentOptions, attempt, defined, effects, inLanguage, issueOf, languageOf, missingConnection, object, planOf, waitOptions } from "./context.js";

const KINDS = { api: "API", ui: "UI" };

const APPROVE = "approve_cases";

const CHANGES = "change_cases";

const KEYWORDS = /^(dado|quando|então|entao|e|mas|cuando|entonces|y|pero|given|when|then|and|but)\s+/i;

const GHERKIN = {
  en: { given: "Given", when: "When", then: "Then", and: "And", scenario: "Scenario" },
  es: { given: "Dado", when: "Cuando", then: "Entonces", and: "Y", scenario: "Escenario" },
  "pt-BR": { given: "Dado", when: "Quando", then: "Então", and: "E", scenario: "Cenário" },
};

function wordsOf(language) {
  return GHERKIN[language] ?? GHERKIN["pt-BR"];
}

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

function gherkin(words, kind, lines) {
  return lines.map((line, index) => `${index === 0 ? words[kind] : words.and} ${line}`).join("\n");
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

function described(entry, language) {
  const one = bdd(entry);
  const words = wordsOf(language);
  const head = `### ${one.ref ? `${one.ref} · ` : ""}${one.title} (${KINDS[one.kind] ?? one.kind})`;
  const parts = [head];
  if (one.objective) parts.push(one.objective);
  const lines = [gherkin(words, "given", one.given), gherkin(words, "when", one.when), gherkin(words, "then", one.then)].filter((text) => text.length > 0);
  parts.push(["```gherkin", `${words.scenario}: ${one.title}`, ...lines.join("\n").split("\n").map((line) => `  ${line}`), "```"].join("\n"));
  return parts.join("\n\n");
}

export function listing(cases, language) {
  if (!Array.isArray(cases) || cases.length === 0) return "No test cases.";
  return cases.map((entry) => described(entry, language)).join("\n\n");
}

const REUSE = "These cases already exist and are linked to the issue. Do not repeat them: write only the cases still missing to cover the acceptance criteria. If none are missing, return an empty list of cases.";

function asked(ctx, opts, issue, feedback, existing) {
  if (opts.prompt) return opts.prompt;
  const plan = planOf(ctx, opts);
  const parts = [`Write the E2E test cases, for the API and the UI, for the item: ${plan.title}`];
  if (issue) parts.push(`Issue: ${issue}`);
  if (plan.story) parts.push(plan.story);
  if (plan.text && plan.text !== plan.story) parts.push(`## Plan\n\n${plan.text}`);
  if (existing.length > 0) parts.push(`## Existing cases\n\n${listing(existing, languageOf(ctx))}\n\n${REUSE}`);
  if (feedback) parts.push(`## Requested changes\n\n${feedback}`);
  return inLanguage(ctx, parts.join("\n\n"));
}

const sameTitle = (a, b) => String(a ?? "").trim().toLowerCase() === String(b ?? "").trim().toLowerCase();

export function missingOnly(cases, existing) {
  return cases.filter((entry) => !existing.some((known) => (entry.ref && entry.ref === known.ref) || sameTitle(entry.title, known.title)));
}

async function drafted(ctx, opts, issue, session, feedback, existing) {
  const input = defined({ prompt: asked(ctx, opts, issue, feedback, existing), session });
  const worktree = opts.worktree ?? ctx.item.branch ?? undefined;
  const result = await ctx.agent(opts.agent ?? "tester", input, agentOptions(opts, { mode: opts.mode ?? "plan", worktree }));
  return { session: result.session, cases: missingOnly(casesOf(result), existing) };
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

const SKIPPED = "skip";

async function linkedCases(ctx, given, issue) {
  if (!issue || given.reuse === false) return [];
  const opts = testsOptions(ctx, given);
  for (;;) {
    const found = await attempt(() => ctx.call("tests", "cases", { issue }, { ...effects(opts), optional: true }));
    if (found !== null) {
      const listed = (Array.isArray(found) ? found : []).map(object).filter((entry) => entry.ref && entry.title);
      return listed.map(bdd);
    }
    const chosen = await missingConnection(ctx, given, "tests", "test cases");
    if (chosen !== "retry") return chosen === SKIPPED ? SKIPPED : ctx.cancel("cancelled for lack of a test case connection");
  }
}

const TOGETHER = 5;

async function created(ctx, given, cases, issue, existing) {
  const opts = testsOptions(ctx, given);
  const issueId = await trackerId(ctx, opts, issue);
  const labels = Array.isArray(opts.labels) ? opts.labels : ["e2e"];
  const words = wordsOf(languageOf(ctx));
  const one = async (entry) => {
    const precondition = gherkin(words, "given", entry.given);
    const args = defined({
      project: opts.project,
      title: entry.title,
      objective: entry.objective,
      precondition: precondition || undefined,
      labels: [...labels, entry.kind === "ui" ? "tela" : "api"],
      steps: [{ action: gherkin(words, "when", entry.when), expected: gherkin(words, "then", entry.then) }],
      folder: opts.folder ?? issue ?? undefined,
      issue: issue ?? undefined,
      issue_id: issueId,
    });
    const found = await ctx.call("tests", "create_case", args, effects(opts));
    return { ...entry, ref: found.ref, url: found.url ?? null };
  };
  const made = cases.length > 0 ? [await one(cases[0])] : [];
  for (let start = 1; start < cases.length; start += TOGETHER) {
    made.push(...(await Promise.all(cases.slice(start, start + TOGETHER).map(one))));
  }
  const title = issue ? `${issue} · ${planOf(ctx, opts).title}` : planOf(ctx, opts).title;
  const cycle = await ctx.call("tests", "create_cycle", defined({ project: opts.project, title, issue: issue ?? undefined, issue_id: issueId }), effects(opts));
  return defined({ cases: [...existing, ...made], reused: existing.length > 0 ? existing.map((entry) => entry.ref) : undefined, cycle: cycle.ref, cycleUrl: cycle.url ?? null });
}

export default step({
  name: "cases",
  label: "Test cases",
  icon: "hNoteEdit",
  requires: "tests",
  doing: "Writing test cases",
  view: (s) => (Array.isArray(s.cases) ? [testCases(s.cases.map(bdd), defined({ session: s.session }))] : s.reason ? [markdown(s.reason)] : []),
  async run(ctx, opts) {
    const issue = issueOf(ctx, opts);
    const existing = await linkedCases(ctx, opts, issue);
    if (existing === SKIPPED) return ctx.skip({ reason: "Step skipped: no test case connection." });
    if (!Array.isArray(existing)) return existing;
    let draft = await drafted(ctx, opts, issue, undefined, undefined, existing);
    for (;;) {
      if (draft.cases.length === 0 && existing.length === 0) return ctx.fail({ session: draft.session, error: "the tester wrote no test cases" });
      const verdict = draft.cases.length === 0 ? { approved: true } : await judged(ctx, opts, { ...draft, cases: [...existing, ...draft.cases] });
      if (verdict.approved) {
        const made = await created(ctx, opts, draft.cases, issue, existing);
        return ctx.pass(defined({ session: draft.session, issue: issue ?? undefined, by: verdict.by, ...made }));
      }
      draft = await drafted(ctx, opts, issue, draft.session, verdict.changes, existing);
    }
  },
});
