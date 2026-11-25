import { step, logs } from "sdk";
import { defined, tail } from "./context.js";

const NOTHING = "No command to run.\n";

async function chosen(ctx, value) {
  return typeof value === "function" ? value(ctx) : value;
}

export default step({
  name: "command",
  label: "Command",
  icon: "sTest",
  doing: "Running commands",
  view: (s) => (Array.isArray(s.ran) && s.ran.length > 0 ? [logs(s.ran)] : [logs(s.output ?? NOTHING)]),
  async run(ctx, opts) {
    const given = await chosen(ctx, opts.run);
    const commands = (Array.isArray(given) ? given : [given]).filter((command) => typeof command === "string" && command);
    if (commands.length === 0) return ctx.skip({ output: NOTHING, ran: [] });
    const repository = await chosen(ctx, opts.repository);
    let output = "";
    const ran = [];
    for (const command of commands) {
      const options = defined({ repository, cwd: opts.cwd, env: opts.env, timeout: opts.timeout });
      const result = await ctx.exec(command, options);
      output += `$ ${command}\n${result.output ?? ""}\n`;
      const passed = result.code === 0;
      ran.push({ name: command, state: passed ? "done" : "failed", seconds: null, log: tail(result.output ?? "", 4000) });
      if (!passed && !opts.keepGoing) return ctx.fail({ output: tail(output), ran, command, code: result.code, error: `${command} exited with ${result.code}` });
    }
    const failed = ran.find((entry) => entry.state === "failed");
    if (failed) return ctx.fail({ output: tail(output), ran, command: failed.name, error: `${failed.name} failed` });
    return ctx.pass({ output: tail(output), ran });
  },
});
