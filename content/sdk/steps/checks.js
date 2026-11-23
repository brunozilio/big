import { step, logs, chat } from "sdk";
import { agentOptions, branchOf, defined, eachOf, planOf, repositoriesOf, tail } from "./context.js";

function groupsOf(ctx, opts) {
  const repositories = repositoriesOf(ctx);
  if (repositories.length === 0) return [{ commands: opts.commands ?? ctx.item.checks ?? [] }];
  return repositories.map((repository) => ({
    repository: repository.id,
    name: repository.name,
    commands: opts.commands ?? (Array.isArray(repository.verify) ? repository.verify : []),
  }));
}

const NOTHING = "Nenhum comando de verificação configurado para este repositório.\n";

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

function checked(s) {
  const results = Array.isArray(s.results) && s.results.length > 0 ? s.results : [s];
  const many = results.length > 1;
  const commands = results.flatMap((result) => (Array.isArray(result.ran) ? result.ran : []).map((entry) => (many && result.name ? { ...entry, name: `${result.name}: ${entry.name}` } : entry)));
  if (s.session) {
    const state = s.verdict === undefined ? "running" : s.approved === false ? "failed" : "done";
    commands.push({ name: "Verificador: critérios de aceite", state, seconds: null, log: s.verdict ?? "" });
  }
  return commands;
}

function asked(ctx, opts, output) {
  const plan = planOf(ctx, opts);
  const parts = [`Verifique o item: ${plan.title}`];
  if (plan.story) parts.push(plan.story);
  if (output) parts.push("## Comandos já rodados\n\n```\n" + tail(output, 3000) + "\n```");
  return parts.join("\n\n");
}

async function verified(ctx, opts, output) {
  const worktree = opts.worktree ?? branchOf(ctx, opts) ?? ctx.item.id;
  const result = await ctx.agent(opts.agent, { prompt: asked(ctx, opts, output) }, agentOptions(opts, { worktree }));
  const text = String(result.text ?? "");
  return { session: result.session, verdict: text, approved: !/VEREDITO:\s*REPROVADO/i.test(text) };
}

function joined(results) {
  if (results.length === 1) return results[0].output;
  return results.map((result) => `# ${result.name}\n${result.output}`).join("\n");
}

export default step({
  name: "checks",
  label: "Checks",
  icon: "sTest",
  doing: "Rodando checks",
  view: (s) => {
    const commands = checked(s);
    if (commands.length === 0) return [logs(s.output ?? NOTHING)];
    return [logs(commands), ...(s.session ? [chat(s.session)] : [])];
  },
  async run(ctx, opts) {
    const groups = groupsOf(ctx, opts);
    const ran = groups.some((group) => group.commands.length > 0);
    const results = ran ? await eachOf(ctx, groups, (branch, group) => verify(branch, opts, group)) : [];
    const output = ran ? tail(joined(results)) : "";
    const listed = groups.length > 1 || groups[0].repository ? { results: results.map((result) => ({ ...result, output: tail(result.output) })) } : {};
    const commands = results.flatMap((result) => result.ran ?? []);
    const failed = results.find((result) => result.command);
    if (failed) return ctx.fail({ output, ran: commands, command: failed.command, code: failed.code, ...listed });
    if (!opts.agent) return ctx.pass({ output: ran ? output : NOTHING, ran: commands, ...listed });
    const check = await verified(ctx, opts, output);
    const data = { output, ran: commands, session: check.session, verdict: check.verdict, approved: check.approved, ...listed };
    if (!check.approved) return ctx.fail({ ...data, error: "o verificador reprovou a entrega" });
    return ctx.pass(data);
  },
});
