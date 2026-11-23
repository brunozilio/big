import { agentOptions, branchOf, defined, eachOf, inLanguage, object, planOf, repositoriesOf, source, tail } from "./context.js";
import { listing } from "./cases.js";

const STATES = ["passed", "failed", "blocked", "not_run"];

const BROKEN = ["failed", "blocked", "not_run"];

export function e2eGroups(ctx, opts) {
  const repositories = repositoriesOf(ctx);
  if (repositories.length === 0) {
    const commands = opts.e2e ?? ctx.item.e2e ?? [];
    return Array.isArray(commands) && commands.length > 0 ? [{ commands }] : [];
  }
  return repositories
    .map((repository) => ({ repository: repository.id, name: repository.name, commands: opts.e2e ?? (Array.isArray(repository.e2e) ? repository.e2e : []) }))
    .filter((group) => group.commands.length > 0);
}

function commandLines(groups) {
  return groups.flatMap((group) => group.commands.map((command) => `- ${group.name ? `${group.name}: ` : ""}\`${command}\``)).join("\n");
}

function asked(ctx, opts, cases, groups) {
  const plan = planOf(ctx, opts);
  const parts = [`Automatize e rode os testes E2E do item: ${plan.title}`, `## Casos de teste\n\n${listing(cases)}`];
  if (groups.length > 0) parts.push(`## Comandos de E2E do repositório\n\n${commandLines(groups)}`);
  const before = object(ctx.output(ctx.step));
  if (before.error) parts.push(`## O que falhou na última vez\n\n${before.error}`);
  return inLanguage(parts.join("\n\n"));
}

function fenced(text) {
  const found = String(text ?? "").match(/```(?:json)?\s*([\s\S]*?)```/);
  if (!found) return {};
  try {
    return object(JSON.parse(found[1]));
  } catch {
    return {};
  }
}

export function verdictsOf(result, cases) {
  const output = object(result.output);
  const reported = Array.isArray(output.results) ? output.results : Array.isArray(fenced(result.text).results) ? fenced(result.text).results : [];
  return cases.map((entry) => {
    const found = object(reported.find((one) => object(one).case === entry.ref || object(one).case === entry.title));
    const status = STATES.includes(found.status) ? found.status : "not_run";
    return { case: entry.ref, title: entry.title, kind: entry.kind, status, evidence: String(found.evidence ?? "") };
  });
}

async function tested(ctx, opts, cases, groups) {
  if (cases.length === 0 || opts.tester === false) return { session: undefined, verdicts: [] };
  const previous = object(object(ctx.output(ctx.step)).e2e).session;
  const worktree = opts.worktree ?? branchOf(ctx, opts) ?? ctx.item.id;
  const input = defined({ prompt: asked(ctx, opts, cases, groups), session: previous });
  const result = await ctx.agent(opts.tester ?? "tester", input, agentOptions(opts, { worktree, mode: opts.testerMode }));
  return { session: result.session, verdicts: verdictsOf(result, cases) };
}

async function recorded(ctx, opts, cycle, verdicts) {
  if (!cycle) return;
  for (const verdict of verdicts) {
    const args = defined({ case: verdict.case, cycle, status: verdict.status, comment: tail(verdict.evidence, 2000) || undefined });
    await ctx.call("tests", "record", args, defined({ connection: opts.tests, retry: opts.retry }));
  }
}

function problem(verdicts, failed) {
  const broken = verdicts.filter((verdict) => BROKEN.includes(verdict.status));
  const lines = broken.map((verdict) => `- ${verdict.case} ${verdict.title}: ${verdict.status}${verdict.evidence ? ` — ${verdict.evidence}` : ""}`);
  if (failed) lines.push(`- o comando \`${failed.command}\` falhou`);
  return lines.length > 0 ? `os testes E2E falharam:\n${lines.join("\n")}` : null;
}

export async function endToEnd(ctx, opts, verify) {
  const found = source(ctx, opts, "cases");
  const cases = (Array.isArray(found.cases) ? found.cases : []).filter((entry) => entry && entry.ref);
  const groups = e2eGroups(ctx, opts);
  if (cases.length === 0 && groups.length === 0) return null;
  const { session, verdicts } = await tested(ctx, opts, cases, groups);
  const results = groups.length > 0 ? await eachOf(ctx, groups, (branch, group) => verify(branch, opts, group)) : [];
  await recorded(ctx, opts, found.cycle, verdicts);
  const failed = results.find((result) => result.command);
  const output = results.map((result) => result.output).join("\n");
  const ran = results.flatMap((result) => (Array.isArray(result.ran) ? result.ran : []).map((entry) => ({ ...entry, name: `E2E: ${entry.name}` })));
  return defined({ session, cycle: found.cycle, cases: verdicts, ran, output: tail(output), error: problem(verdicts, failed) });
}

export function e2eLines(e2e) {
  const found = object(e2e);
  const ran = Array.isArray(found.ran) ? found.ran : [];
  const cases = (Array.isArray(found.cases) ? found.cases : []).map((verdict) => ({
    name: `Caso ${verdict.case}: ${verdict.title}`,
    state: verdict.status === "passed" ? "done" : "failed",
    seconds: null,
    log: verdict.evidence ?? "",
  }));
  return [...ran, ...cases];
}
