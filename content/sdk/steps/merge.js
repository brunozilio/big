import { step, merge as merging } from "sdk";
import { codeOptions, eachOf, effects, object, pullRequestsOf, targetFields, waitOptions } from "./context.js";

const METHODS = ["squash", "merge", "rebase"];

function ordered(opts) {
  const methods = opts.methods ?? METHODS;
  return opts.method ? [opts.method, ...methods.filter((method) => method !== opts.method)] : methods;
}

function shown(s) {
  const listed = Array.isArray(s.merges) && s.merges.length > 0 ? s.merges : s.pr ? [s] : [];
  return listed.map((entry) => merging(entry.pr, s.methods ?? METHODS));
}

async function mergeOne(ctx, opts, entry, method) {
  const merged = await ctx.call("code", "merge", { pr: entry.pr, method }, effects(codeOptions(opts, entry)));
  return { ...targetFields(entry), pr: entry.pr, merged: merged?.merged === true, sha: merged?.sha ?? null };
}

export default step({
  name: "merge",
  label: "Merge",
  icon: "sMerge",
  needs: "code",
  doing: "Merging",
  view: shown,
  async run(ctx, opts) {
    const prs = pullRequestsOf(ctx, opts);
    if (prs.length === 0) return ctx.fail({ error: "there is no pull request to merge" });
    const pr = prs[0].pr;
    const methods = ordered(opts);
    let method = methods[0];
    if (opts.manual !== false) {
      const answer = await ctx.waitHuman(prs.map((entry) => merging(entry.pr, methods)), waitOptions(opts));
      const chosen = object(answer.values).method ?? answer.action;
      if (methods.includes(chosen)) method = chosen;
    }
    const merges = await eachOf(ctx, prs, (branch, entry) => mergeOne(branch, opts, entry, method));
    const listed = prs.length > 1 || prs[0].repository ? { merges } : {};
    if (merges.some((done) => !done.merged)) {
      return ctx.fail({ pr, method, methods, ...listed, error: "the pull request was not merged" });
    }
    return ctx.pass({ pr, method, methods, merged: true, sha: merges[0].sha, ...listed });
  },
});
