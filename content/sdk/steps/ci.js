import { step, pipeline } from "sdk";
import { FINISHED, SUCCEEDED, branchOf, failedLogs, findRun, object, pullRequestOf, settle } from "./context.js";

export default step({
  name: "ci",
  label: "CI",
  icon: "sCi",
  needs: "pipelines",
  doing: "Aguardando CI",
  view: (s) => (s.run ? [pipeline(s.run)] : []),
  async run(ctx, opts) {
    const ref = opts.ref ?? pullRequestOf(ctx, opts)?.head ?? branchOf(ctx, opts);
    if (!ref) return ctx.fail({ error: "não há branch nem pull request para acompanhar o pipeline" });
    const seen = object(ctx.output(ctx.step)).seen ?? [];
    const found = await findRun(ctx, opts, { ref, seen });
    const run = await settle(ctx, opts, found.ref, FINISHED, ["pipelines.run.finished"]);
    const data = { run: run.ref, state: run.state, url: run.url ?? null, seen: [...seen, run.ref] };
    if (SUCCEEDED.includes(run.state)) return ctx.pass(data);
    const output = await failedLogs(ctx, opts, run);
    return ctx.fail({ ...data, summary: `O pipeline ${run.pipeline ?? run.ref} terminou como ${run.state}.`, output });
  },
});
