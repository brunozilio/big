import { step, pipeline } from "sdk";
import { FINISHED, SUCCEEDED, branchOf, eachOf, failedLogs, findRun, object, pipelineOptions, pullRequestsOf, repoOf, settle, targetFields } from "./context.js";

function targetsOf(ctx, opts) {
  const prs = pullRequestsOf(ctx, opts);
  if (opts.ref || prs.length === 0) return [{ ...(prs[0] ?? {}), head: opts.ref ?? prs[0]?.head ?? branchOf(ctx, opts) }];
  return prs.map((entry) => ({ ...entry, head: entry.head ?? branchOf(ctx, opts) }));
}

async function follow(ctx, opts, target, seen) {
  const options = pipelineOptions(opts, target);
  const found = await findRun(ctx, options, { ref: target.head, repo: repoOf(target), seen });
  const run = await settle(ctx, options, found.ref, FINISHED, ["pipelines.run.finished"]);
  const data = { ...targetFields(target), run: run.ref, state: run.state, url: run.url ?? null };
  if (SUCCEEDED.includes(run.state)) return data;
  const output = await failedLogs(ctx, options, run);
  return { ...data, pipeline: run.pipeline ?? run.ref, output };
}

function failure(failed) {
  const summary = failed.map((run) => `O pipeline ${run.pipeline}${run.name ? ` de ${run.name}` : ""} terminou como ${run.state}.`);
  const output = failed.map((run) => (failed.length > 1 && run.name ? `# ${run.name}\n${run.output}` : run.output));
  return { summary: summary.join("\n"), output: output.join("\n\n") };
}

export default step({
  name: "ci",
  label: "CI",
  icon: "sCi",
  needs: "pipelines",
  doing: "Aguardando CI",
  view: (s) => (Array.isArray(s.runs) && s.runs.length > 0 ? s.runs : s.run ? [s] : []).map((run) => pipeline(run.run)),
  async run(ctx, opts) {
    const targets = targetsOf(ctx, opts);
    if (targets.some((target) => !target.head)) {
      return ctx.fail({ error: "não há branch nem pull request para acompanhar o pipeline" });
    }
    const seen = object(ctx.output(ctx.step)).seen ?? [];
    const runs = await eachOf(ctx, targets, (branch, target) => follow(branch, opts, target, seen));
    const first = runs[0];
    const data = { run: first.run, state: first.state, url: first.url, seen: [...seen, ...runs.map((run) => run.run)] };
    const listed = runs.length > 1 || first.repository ? { ...data, runs: runs.map(({ output: _o, pipeline: _p, ...run }) => run) } : data;
    const failed = runs.filter((run) => !SUCCEEDED.includes(run.state));
    if (failed.length === 0) return ctx.pass(listed);
    return ctx.fail({ ...listed, state: failed[0].state, ...failure(failed) });
  },
});
