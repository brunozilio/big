import { step, merge as merging } from "sdk";
import { effects, object, pullRequestOf, waitOptions } from "./context.js";

const METHODS = ["squash", "merge", "rebase"];

function ordered(opts) {
  const methods = opts.methods ?? METHODS;
  return opts.method ? [opts.method, ...methods.filter((method) => method !== opts.method)] : methods;
}

export default step({
  name: "merge",
  view: (s) => (s.pr ? [merging(s.pr, s.methods ?? METHODS)] : []),
  async run(ctx, opts) {
    const pr = pullRequestOf(ctx, opts)?.pr;
    if (!pr) return ctx.fail({ error: "não há pull request para mergear" });
    const methods = ordered(opts);
    let method = methods[0];
    if (opts.manual !== false) {
      const answer = await ctx.waitHuman([merging(pr, methods)], waitOptions(opts));
      const chosen = object(answer.values).method ?? answer.action;
      if (methods.includes(chosen)) method = chosen;
    }
    const merged = await ctx.call("code", "merge", { pr, method }, effects(opts));
    if (!merged || merged.merged !== true) return ctx.fail({ pr, method, methods, error: "o pull request não foi mergeado" });
    return ctx.pass({ pr, method, methods, sha: merged.sha ?? null });
  },
});
