import { agentOptions, branchOf, defined, eachOf, inLanguage, object, planOf, repositoriesOf, source, tail } from "./context.js";
import { listing } from "./cases.js";

const STATES = ["passed", "failed", "blocked", "not_run"];

const BROKEN = ["failed", "blocked", "not_run"];

export function e2eGroups(ctx, opts) {
  const commands = opts.e2e ?? [];
  if (!Array.isArray(commands) || commands.length === 0) return [];
  const repositories = repositoriesOf(ctx);
  if (repositories.length === 0) return [{ commands }];
  return repositories.map((repository) => ({ repository: repository.id, name: repository.name, commands }));
}

function commandLines(groups) {
  return groups.flatMap((group) => group.commands.map((command) => `- ${group.name ? `${group.name}: ` : ""}\`${command}\``)).join("\n");
}

function asked(ctx, opts, cases, groups) {
  const plan = planOf(ctx, opts);
  const keys = cases.map((entry) => `\`${entry.ref}\``).join(", ");
  const parts = [`Automate and run the E2E tests for the item: ${plan.title}`, `## Test cases\n\n${listing(cases)}`];
  if (keys) parts.push(`In \`results\`, each result's \`case\` field is only the case key, without the title: ${keys}.`);
  if (groups.length > 0) parts.push(`## Repository E2E commands\n\n${commandLines(groups)}`);
  const before = object(ctx.output(ctx.step));
  if (before.error) parts.push(`## What failed last time\n\n${before.error}`);
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

const plain = (text) => String(text ?? "").trim().toLowerCase();

const TOKENS = /[\p{L}\p{N}_-]+/gu;

const tokensOf = (text) => plain(text).match(TOKENS) ?? [];

const leads = (text, ref) => text === ref || (text.startsWith(ref) && !/[\p{L}\p{N}_-]/u.test(text.charAt(ref.length)));

const RULES = [
  (text, entry) => text === plain(entry.ref),
  (text, entry) => leads(text, plain(entry.ref)),
  (text, entry) => text === plain(entry.title),
  (text, entry) => text.endsWith(plain(entry.title)) && plain(entry.title).length > 0,
  (text, entry) => tokensOf(text).includes(plain(entry.ref)),
];

export function caseOf(reported, cases) {
  const text = plain(reported);
  if (!text) return null;
  for (const rule of RULES) {
    const found = cases.find((entry) => entry.ref && rule(text, entry));
    if (found) return found;
  }
  return null;
}

export function matched(reported, cases) {
  const byCase = new Map();
  const unknown = [];
  for (const one of reported.map(object)) {
    const entry = caseOf(one.case, cases);
    if (!entry) unknown.push(String(one.case ?? ""));
    else if (!byCase.has(entry.ref)) byCase.set(entry.ref, one);
  }
  return { byCase, unknown };
}

function reportedOf(result) {
  const output = object(result.output);
  if (Array.isArray(output.results)) return output.results;
  const parsed = fenced(result.text).results;
  return Array.isArray(parsed) ? parsed : [];
}

export function verdictsOf(result, cases) {
  const { byCase, unknown } = matched(reportedOf(result), cases);
  const stray = unknown.length > 0 ? `the tester reported an unknown case key: ${unknown.map((key) => `"${key}"`).join(", ")}` : "";
  return cases.map((entry) => {
    const found = object(byCase.get(entry.ref));
    const status = STATES.includes(found.status) ? found.status : "not_run";
    const files = (Array.isArray(found.files) ? found.files : []).filter((file) => typeof file === "string" && file.length > 0);
    const evidence = String(found.evidence ?? "") || (byCase.has(entry.ref) ? "" : stray);
    return { case: entry.ref, title: entry.title, kind: entry.kind, status, evidence, output: tail(found.output ?? "", 8000), files };
  });
}

function places(ctx) {
  const repositories = repositoriesOf(ctx);
  return repositories.length === 0 ? [undefined] : repositories.map((repository) => repository.id);
}

export async function kept(ctx, opts, verdicts) {
  const wanted = [...new Set(verdicts.flatMap((verdict) => verdict.files))];
  if (wanted.length === 0) return verdicts.map((verdict) => ({ ...verdict, files: [] }));
  const found = new Map();
  for (const repository of places(ctx)) {
    const missing = wanted.filter((path) => !found.has(path));
    if (missing.length === 0) break;
    const result = await ctx.exec(":", defined({ repository, keep: missing, retry: opts.retry }));
    for (const file of Array.isArray(result.kept) ? result.kept : []) found.set(file.path, { id: file.id, name: file.name, size: file.size });
  }
  return verdicts.map((verdict) => ({ ...verdict, files: verdict.files.map((path) => found.get(path)).filter(Boolean) }));
}

export function proof(verdict) {
  return [verdict.evidence, verdict.output].filter((text) => text && String(text).trim().length > 0).join("\n\n");
}

async function tested(ctx, opts, cases, groups) {
  if (cases.length === 0 || opts.tester === false) return { session: undefined, verdicts: [] };
  const previous = object(object(ctx.output(ctx.step)).e2e).session;
  const worktree = opts.worktree ?? branchOf(ctx, opts) ?? ctx.item.id;
  const input = defined({ prompt: asked(ctx, opts, cases, groups), session: previous });
  const result = await ctx.agent(opts.tester ?? "tester", input, agentOptions(opts, { worktree, mode: opts.testerMode }));
  return { session: result.session, verdicts: await kept(ctx, opts, verdictsOf(result, cases)) };
}

async function recorded(ctx, opts, cycle, verdicts) {
  if (!cycle) return;
  for (const verdict of verdicts) {
    const args = defined({ case: verdict.case, cycle, status: verdict.status, comment: tail(proof(verdict), 4000) || undefined });
    await ctx.call("tests", "record", args, defined({ connection: opts.tests, retry: opts.retry }));
  }
}

function problem(verdicts, failed) {
  const broken = verdicts.filter((verdict) => BROKEN.includes(verdict.status));
  const lines = broken.map((verdict) => `- ${verdict.case} ${verdict.title}: ${verdict.status}${verdict.evidence ? ` — ${verdict.evidence}` : ""}`);
  if (failed) lines.push(`- command \`${failed.command}\` failed`);
  return lines.length > 0 ? `the E2E tests failed:\n${lines.join("\n")}` : null;
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
    name: `Case ${verdict.case}: ${verdict.title}`,
    state: verdict.status === "passed" ? "done" : "failed",
    seconds: null,
    log: proof(verdict),
    files: Array.isArray(verdict.files) ? verdict.files : [],
  }));
  const tester = found.session ? [{ name: "E2E agent", state: cases.some((entry) => entry.state === "failed") ? "failed" : "done", seconds: null, log: "", session: found.session }] : [];
  return [...tester, ...ran, ...cases];
}
