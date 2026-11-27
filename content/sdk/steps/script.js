import { step, logs } from "sdk";
import { defined, tail } from "./context.js";

async function chosen(ctx, value) {
  return typeof value === "function" ? value(ctx) : value;
}

export default step({
  name: "script",
  label: "Script",
  icon: "terminal",
  doing: "Running a script",
  view: (s) => [logs(s.output ?? "")],
  async run(ctx, opts) {
    const name = await chosen(ctx, opts.name);
    if (typeof name !== "string" || name === "") return ctx.fail({ error: "script needs the name of a script of the bank" });
    const input = (await chosen(ctx, opts.input)) ?? {};
    const repository = await chosen(ctx, opts.repository);
    const options = defined({ repository, pure: opts.pure === true ? true : undefined, retry: opts.retry, timeout: opts.timeout });
    const result = await ctx.script(name, input, options);
    return ctx.pass({ name, result, output: tail(JSON.stringify(result, null, 2) ?? "null") });
  },
});
