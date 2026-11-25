import { step, logs, chat, markdown } from "sdk";
import { agentOptions, branchOf, defined, inLanguage, object, planOf, repositoriesOf, tail } from "./context.js";
import { endToEnd, e2eLines } from "./e2e.js";

export const NOTHING = "No verification command configured for this repository.\n";

const NODE = ["id", "needs", "if", "when", "onFail", "retries", "rerun", "forEach", "concurrency", "failFast", "status", "doneStatus", "label", "icon", "doing"];

export function own(opts) {
  return Object.fromEntries(Object.entries(opts).filter(([key]) => !NODE.includes(key)));
}

export function groupsOf(ctx, opts) {
  const repositories = repositoriesOf(ctx);
  if (repositories.length === 0) return [{ commands: opts.commands ?? ctx.item.checks ?? [] }];
  return repositories.map((repository) => ({
    repository: repository.id,
    name: repository.name,
    commands: opts.commands ?? (Array.isArray(repository.verify) ? repository.verify : []),
  }));
}

async function verify(ctx, opts, group) {
  if (group.commands.length === 0) return defined({ repository: group.repository, name: group.name, output: NOTHING, ran: [] });
  let output = "";
  const ran = [];
  for (const command of group.commands) {
    const options = defined({ repository: group.repository, cwd: opts.cwd, env: opts.env, timeout: opts.timeout, retry: opts.retry });
    const result = await ctx.exec(command, options);
    output += `$ ${command}\n${result.output ?? ""}\n`;
    const passed = result.code === 0;
    ran.push({ name: command, state: passed ? "done" : "failed", seconds: null, log: tail(result.output ?? "", 4000) });
    if (!passed) return defined({ repository: group.repository, name: group.name, output, command, code: result.code, ran });
  }
  return defined({ repository: group.repository, name: group.name, output, ran });
}

export function checked(s) {
  const results = Array.isArray(s.results) && s.results.length > 0 ? s.results : [s];
  const many = results.length > 1;
  const commands = results.flatMap((result) => (Array.isArray(result.ran) ? result.ran : []).map((entry) => (many && result.name ? { ...entry, name: `${result.name}: ${entry.name}` } : entry)));
  commands.push(...e2eLines(s.e2e));
  const verifying = s.state === undefined || s.state === "running";
  if (s.session && (s.verdict !== undefined || verifying)) {
    const state = s.verdict === undefined ? "running" : s.approved === false ? "failed" : "done";
    commands.push({ name: "Verifier: acceptance criteria", state, seconds: null, log: s.verdict ?? "", session: s.session });
  }
  return commands;
}

function asked(ctx, opts, output) {
  const plan = planOf(ctx, opts);
  const parts = [`Verify the item: ${plan.title}`];
  if (plan.story) parts.push(plan.story);
  if (output) parts.push("## Commands already run\n\n```\n" + tail(output, 3000) + "\n```");
  return inLanguage(ctx, parts.join("\n\n"));
}

function joined(results) {
  if (results.length === 1) return results[0].output;
  return results.map((result) => `# ${result.name}\n${result.output}`).join("\n");
}

function lanesOf(value) {
  if (Array.isArray(value)) return value.filter(Boolean);
  const found = object(value);
  return Array.isArray(found.results) ? found.results.filter(Boolean) : [];
}

function commandsOf(steps) {
  const results = lanesOf(steps.commands);
  const ran = results.some((result) => Array.isArray(result.ran) && result.ran.length > 0);
  const output = ran ? tail(joined(results)) : "";
  const listed = results.length > 1 || results[0]?.repository ? { results: results.map((result) => ({ ...result, output: tail(result.output) })) } : {};
  const commands = results.flatMap((result) => result.ran ?? []);
  return { results, ran, output, listed, commands };
}

export function collect(steps, _result, opts) {
  const { results, ran, output, listed, commands } = commandsOf(steps);
  const failed = results.find((result) => result.command);
  if (failed) return { output, ran: commands, command: failed.command, code: failed.code, ...listed };
  const e2e = steps.e2e && typeof steps.e2e === "object" ? steps.e2e : null;
  const both = e2e && e2e.output ? tail(`${output}\n${e2e.output}`) : output;
  const extra = e2e ? { e2e } : {};
  if (e2e && e2e.error) return { output: both, ran: commands, error: e2e.error, ...listed, ...extra };
  const check = object(steps.verifier);
  if (!opts.agent || check.session === undefined) return { output: ran || e2e ? both : NOTHING, ran: commands, ...listed, ...extra };
  const data = { output: both, ran: commands, session: check.session, verdict: check.verdict, approved: check.approved, ...listed, ...extra };
  return check.approved ? data : { ...data, error: "the verifier rejected the delivery" };
}

export const repoChecks = step({
  name: "repo-checks",
  label: "Commands",
  icon: "sTest",
  doing: "Running checks",
  view: (s) => (Array.isArray(s.ran) && s.ran.length > 0 ? [logs(s.ran)] : [logs(s.output ?? NOTHING)]),
  async run(ctx, opts) {
    const result = await verify(ctx, opts, ctx.each ? ctx.each.value : groupsOf(ctx, opts)[0]);
    return result.command ? ctx.fail(result) : ctx.pass(result);
  },
});

export const e2e = step({
  name: "e2e",
  label: "E2E",
  icon: "sTest",
  doing: "Running E2E tests",
  view: (s) => [logs(e2eLines(s))],
  async run(ctx, opts) {
    const result = await endToEnd(ctx, opts, verify);
    if (!result) return ctx.skip(null);
    return result.error ? ctx.fail(result) : ctx.pass(result);
  },
});

export const verifier = step({
  name: "verifier",
  label: "Verifier",
  icon: "sTest",
  doing: "Verifying the delivery",
  view: (s) => (s.session ? [chat(s.session)] : s.verdict ? [markdown(s.verdict)] : []),
  async run(ctx, opts) {
    const { output } = commandsOf({ commands: ctx.output("commands") });
    const tested = object(ctx.output("e2e"));
    const both = tested.output ? tail(`${output}\n${tested.output}`) : output;
    const worktree = opts.worktree ?? branchOf(ctx, opts) ?? ctx.item.id;
    const result = await ctx.agent(opts.agent, { prompt: asked(ctx, opts, both) }, agentOptions(opts, { worktree }));
    const text = String(result.text ?? "");
    const approved = !/(?:VERDICT|VEREDITO|VEREDICTO):\s*(?:REJECTED|REPROVADO|RECHAZADO)/i.test(text);
    const data = { session: result.session, verdict: text, approved };
    return approved ? ctx.pass(data) : ctx.fail({ ...data, error: "the verifier rejected the delivery" });
  },
});
