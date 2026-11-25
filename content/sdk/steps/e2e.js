import { projectOf } from "sdk/bindings.js";
import { agentOptions, attempt, branchOf, defined, eachOf, inLanguage, issueOf, languageOf, object, planOf, repositoriesOf, source, tail } from "./context.js";
import { listing } from "./cases.js";

const STATES = ["passed", "failed", "blocked", "not_run"];

const BROKEN = ["failed", "blocked", "not_run"];

function suiteOf(repository) {
  const commands = (Array.isArray(repository.e2e) ? repository.e2e : []).map((line) => String(line ?? "").trim()).filter(Boolean);
  const cwd = String(repository.e2eDirectory ?? "").trim() || undefined;
  return defined({ repository: repository.id, name: repository.name, commands, cwd });
}

export function e2eGroups(ctx, opts) {
  const repositories = repositoriesOf(ctx);
  const commands = Array.isArray(opts.e2e) ? opts.e2e : [];
  const declared = repositories.map(suiteOf).filter((group) => group.commands.length > 0);
  if (declared.length > 0 && commands.length === 0) return declared;
  if (commands.length === 0) return [];
  if (repositories.length === 0) return [defined({ commands, cwd: opts.cwd })];
  return repositories.map((repository) => defined({ repository: repository.id, name: repository.name, commands, cwd: opts.cwd }));
}

function commandLines(groups) {
  const place = (group) => [group.name, group.cwd].filter(Boolean).join("/");
  return groups.flatMap((group) => group.commands.map((command) => `- ${place(group) ? `${place(group)}: ` : ""}\`${command}\``)).join("\n");
}

function asked(ctx, opts, cases, groups) {
  const plan = planOf(ctx, opts);
  const keys = cases.map((entry) => `\`${entry.ref}\``).join(", ");
  const parts = [`Automate and run the E2E tests for the item: ${plan.title}`, `## Test cases\n\n${listing(cases, languageOf(ctx))}`];
  if (keys) parts.push(`In \`results\`, each result's \`case\` field is only the case key, without the title: ${keys}.`);
  if (groups.length > 0) parts.push(`## Repository E2E commands\n\nAutomate the cases in the suite these commands run, in its repository and folder.\n\n${commandLines(groups)}`);
  const before = object(ctx.output(ctx.step));
  if (before.error) parts.push(`## What failed last time\n\n${before.error}`);
  return inLanguage(ctx, parts.join("\n\n"));
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
  const before = object(ctx.output(ctx.step));
  const previous = (before.e2e ? object(before.e2e) : before).session;
  const worktree = opts.worktree ?? branchOf(ctx, opts) ?? ctx.item.id;
  const input = defined({ prompt: asked(ctx, opts, cases, groups), session: previous });
  const result = await ctx.agent(opts.tester ?? "tester", input, agentOptions(opts, { worktree, mode: opts.testerMode }));
  return { session: result.session, verdicts: await kept(ctx, opts, verdictsOf(result, cases)) };
}

const REPORT = {
  en: { title: "E2E tests", cycle: "cycle", files: "Evidence attached", passed: "passed", failed: "failed", blocked: "blocked", not_run: "not run" },
  es: { title: "Pruebas E2E", cycle: "ciclo", files: "Evidencias adjuntas", passed: "aprobado", failed: "falló", blocked: "bloqueado", not_run: "no ejecutado" },
  "pt-BR": { title: "Testes E2E", cycle: "ciclo", files: "Evidências anexadas", passed: "passou", failed: "falhou", blocked: "bloqueado", not_run: "não executado" },
};

const MARKS = { passed: "✅", failed: "❌", blocked: "⛔", not_run: "⏭️" };

const failure = (error) => String(error?.message ?? error ?? "");

async function recorded(ctx, opts, found, verdicts) {
  if (!found.cycle || verdicts.length === 0) return { verdicts, notes: [] };
  const where = defined({ connection: opts.tests, retry: opts.retry });
  const done = [];
  for (const verdict of verdicts) {
    const args = defined({ case: verdict.case, cycle: found.cycle, status: verdict.status, comment: tail(proof(verdict), 4000) || undefined, issue_id: found.issueId });
    const made = object(await ctx.call("tests", "record", args, where));
    done.push(defined({ ...verdict, execution: made.ref || undefined }));
  }
  const status = done.every((verdict) => verdict.status === "passed") ? "done" : "in_progress";
  const finished = await attempt(() => ctx.call("tests", "finish_cycle", { cycle: found.cycle, status }, where));
  return { verdicts: done, notes: finished === undefined ? [`the cycle ${found.cycle} kept its status`] : [] };
}

function summaryOf(ctx, cycle, verdicts, attached) {
  const words = REPORT[languageOf(ctx)] ?? REPORT["pt-BR"];
  const head = `**${words.title}**${cycle ? ` · ${words.cycle} ${cycle}` : ""}`;
  const lines = verdicts.map((verdict) => {
    const execution = verdict.execution ? ` (${verdict.execution})` : "";
    const evidence = verdict.status === "passed" || !verdict.evidence ? "" : ` — ${verdict.evidence}`;
    return `- ${MARKS[verdict.status] ?? ""} ${verdict.case} ${verdict.title}: ${words[verdict.status] ?? verdict.status}${execution}${evidence}`;
  });
  const files = attached.length > 0 ? [`${words.files}: ${attached.join(", ")}`] : [];
  return [head, lines.join("\n"), ...files].join("\n\n");
}

async function attached(ctx, issue, verdicts, where) {
  const byId = new Map(verdicts.flatMap((verdict) => verdict.files).map((file) => [file.id, file]));
  if (byId.size === 0) return { names: [], notes: [] };
  try {
    const files = [...byId.keys()].map((id) => ({ $file: id }));
    const done = object(await ctx.call("tracker", "attach", { issue, files }, where));
    const skipped = Array.isArray(done.skipped) ? done.skipped : [];
    return { names: Array.isArray(done.attached) ? done.attached : [], notes: skipped.map((file) => `not attached to ${issue}: ${file}`) };
  } catch (error) {
    return { names: [], notes: [`the evidence was not attached to ${issue}: ${failure(error)}`] };
  }
}

async function reported(ctx, opts, cycle, verdicts) {
  const issue = issueOf(ctx, opts);
  if (!issue || verdicts.length === 0 || opts.report === false) return [];
  const where = defined({ connection: opts.tracker ?? projectOf(ctx.item.application, "tracker").connection, retry: opts.retry });
  const files = await attached(ctx, issue, verdicts, where);
  try {
    await ctx.call("tracker", "comment", { issue, body: summaryOf(ctx, cycle, verdicts, files.names) }, where);
    return files.notes;
  } catch (error) {
    return [...files.notes, `the summary was not posted on ${issue}: ${failure(error)}`];
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
  const tried = await tested(ctx, opts, cases, groups);
  const session = tried.session;
  const results = groups.length > 0 ? await eachOf(ctx, groups, (branch, group) => verify(branch, opts, group)) : [];
  const logged = await recorded(ctx, opts, found, tried.verdicts);
  const verdicts = logged.verdicts;
  const notes = [...logged.notes, ...(await reported(ctx, opts, found.cycle, verdicts))];
  const failed = results.find((result) => result.command);
  const output = results.map((result) => result.output).join("\n");
  const ran = results.flatMap((result) => (Array.isArray(result.ran) ? result.ran : []).map((entry) => ({ ...entry, name: `E2E: ${entry.name}` })));
  const shown = notes.length > 0 ? notes : undefined;
  return defined({ session, cycle: found.cycle, cases: verdicts, ran, output: tail(output), notes: shown, error: problem(verdicts, failed) });
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
  const notes = Array.isArray(found.notes) && found.notes.length > 0 ? [{ name: "Report", state: "done", seconds: null, log: found.notes.join("\n") }] : [];
  return [...tester, ...ran, ...cases, ...notes];
}
