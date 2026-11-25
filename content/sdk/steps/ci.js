import { step, pipeline, markdown } from "sdk";

const NONE = "none";
import { FINISHED, SUCCEEDED, branchOf, eachOf, failedLogs, findRun, object, pipelineOptions, pullRequestsOf, repoOf, settle, targetFields } from "./context.js";

function targetsOf(ctx, opts) {
  const prs = pullRequestsOf(ctx, opts);
  if (opts.ref || prs.length === 0) return [{ ...(prs[0] ?? {}), head: opts.ref ?? prs[0]?.head ?? branchOf(ctx, opts) }];
  return prs.map((entry) => ({ ...entry, head: entry.head ?? branchOf(ctx, opts) }));
}

async function follow(ctx, opts, target, seen) {
  const options = pipelineOptions(opts, target);
  const found = await findRun(ctx, options, { ref: target.head, repo: repoOf(target), seen, optional: true });
  if (!found) return { ...targetFields(target), run: null, state: NONE, url: null, head: target.head };
  const run = await settle(ctx, options, found.ref, FINISHED, ["pipelines.run.finished"]);
  const data = { ...targetFields(target), run: run.ref, state: run.state, url: run.url ?? null };
  if (SUCCEEDED.includes(run.state)) return data;
  const output = await failedLogs(ctx, options, run);
  return { ...data, pipeline: run.pipeline ?? run.ref, output };
}

function shown(run) {
  if (run.state === NONE) return markdown("No CI pipeline configured.");
  return pipeline(run.run);
}

function failure(failed) {
  const summary = failed.map((run) => `Pipeline ${run.pipeline}${run.name ? ` for ${run.name}` : ""} finished as ${run.state}.`);
  const output = failed.map((run) => (failed.length > 1 && run.name ? `# ${run.name}\n${run.output}` : run.output));
  return { summary: summary.join("\n"), output: output.join("\n\n") };
}

export default step({
  name: "ci",
  label: "CI",
  icon: "sCi",
  requires: "pipelines",
  doing: "Waiting for CI",
  view: (s) => (Array.isArray(s.runs) && s.runs.length > 0 ? s.runs : [s]).filter((run) => run.run || run.state === NONE).map(shown),
  async run(ctx, opts) {
    const targets = targetsOf(ctx, opts);
    if (targets.some((target) => !target.head)) {
      return ctx.fail({ error: "there is no branch or pull request to follow the pipeline" });
    }
    const seen = object(ctx.output(ctx.step)).seen ?? [];
    const runs = await eachOf(ctx, targets, (branch, target) => follow(branch, opts, target, seen));
    const first = runs[0];
    const data = { run: first.run, state: first.state, url: first.url, head: first.head, seen: [...seen, ...runs.filter((run) => run.run).map((run) => run.run)] };
    const listed = runs.length > 1 || first.repository ? { ...data, runs: runs.map(({ output: _o, pipeline: _p, ...run }) => run) } : data;
    const failed = runs.filter((run) => run.state !== NONE && !SUCCEEDED.includes(run.state));
    if (runs.every((run) => run.state === NONE)) return ctx.skip(listed);
    if (failed.length === 0) return ctx.pass(listed);
    return ctx.fail({ ...listed, state: failed[0].state, ...failure(failed) });
  },
});
