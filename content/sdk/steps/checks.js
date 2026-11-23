import { step, logs } from "sdk";
import { defined, tail } from "./context.js";

export default step({
  name: "checks",
  view: (s) => [logs(s.output ?? "")],
  async run(ctx, opts) {
    const commands = opts.commands ?? ctx.item.checks ?? [];
    let output = "";
    for (const command of commands) {
      const options = defined({ cwd: opts.cwd, env: opts.env, timeout: opts.timeout, retry: opts.retry });
      const result = await ctx.exec(command, options);
      output += `$ ${command}\n${result.output ?? ""}\n`;
      if (result.code !== 0) return ctx.fail({ output: tail(output), command, code: result.code });
    }
    return ctx.pass({ output: tail(output) });
  },
});
