import { step, logs } from "sdk";
import { defined, eachOf, repositoriesOf, tail } from "./context.js";

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
  if (group.commands.length === 0) return defined({ repository: group.repository, name: group.name, output: NOTHING });
  let output = "";
  for (const command of group.commands) {
    const options = defined({ repository: group.repository, cwd: opts.cwd, env: opts.env, timeout: opts.timeout, retry: opts.retry });
    const result = await ctx.exec(command, options);
    output += `$ ${command}\n${result.output ?? ""}\n`;
    if (result.code !== 0) return defined({ repository: group.repository, name: group.name, output, command, code: result.code });
  }
  return defined({ repository: group.repository, name: group.name, output });
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
  view: (s) => [logs(s.output ?? "")],
  async run(ctx, opts) {
    const groups = groupsOf(ctx, opts);
    const results = await eachOf(ctx, groups, (branch, group) => verify(branch, opts, group));
    const output = tail(joined(results));
    const listed = groups.length > 1 || groups[0].repository ? { results: results.map((result) => ({ ...result, output: tail(result.output) })) } : {};
    const failed = results.find((result) => result.command);
    if (failed) return ctx.fail({ output, command: failed.command, code: failed.code, ...listed });
    return ctx.pass({ output, ...listed });
  },
});
