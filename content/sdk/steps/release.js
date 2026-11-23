import { step, actions, markdown, pipeline } from "sdk";
import { FINISHED, SUCCEEDED, effects, findRun, object, pullRequestOf, settle, source, waitOptions } from "./context.js";

const WAITING = [...FINISHED, "waiting"];

const environments = (pending) => pending.map((deployment) => deployment.environment).join(", ");

async function decide(ctx, opts, run, pending) {
  if (opts.approve === "auto") return "approve";
  const view = [
    pipeline(run),
    markdown(`Deploy aguardando aprovação: ${environments(pending)}`),
    actions(["approve_deploy", "reject_deploy"]),
  ];
  const raced = await ctx.race([
    (branch) => branch.waitHuman(view, waitOptions(opts)),
    (branch) => settle(branch, opts, run, FINISHED, ["pipelines.run.finished"]),
  ]);
  if (raced.index === 1) return "outside";
  return raced.value.action === "approve_deploy" ? "approve" : "reject";
}

async function gate(ctx, opts, run) {
  const listed = await ctx.call("pipelines", "pending_deployments", { run }, effects(opts));
  const pending = (Array.isArray(listed) ? listed : []).filter((deployment) => deployment.canApprove !== false);
  if (pending.length === 0) return { decision: null, pending };
  const decision = await decide(ctx, opts, run, pending);
  if (decision === "outside") return { decision, pending };
  const deployments = pending.map((deployment) => deployment.id);
  await ctx.call("pipelines", "review_deployment", { run, deployments, decision }, effects(opts));
  return { decision, pending };
}

export default step({
  name: "release",
  label: "Release",
  icon: "sRelease",
  needs: "pipelines",
  doing: "Deploy",
  view: (s) => (s.run ? [pipeline(s.run)] : []),
  async run(ctx, opts) {
    const ref = opts.ref ?? source(ctx, opts, "merge").sha ?? pullRequestOf(ctx, opts)?.base;
    if (!ref) return ctx.fail({ error: "não há commit nem branch para acompanhar a release" });
    const seen = object(ctx.output(ctx.step)).seen ?? [];
    const found = await findRun(ctx, opts, { ref, seen });
    const data = { run: found.ref, url: found.url ?? null, seen: [...seen, found.ref] };
    for (;;) {
      const events = ["pipelines.run.finished", "pipelines.deployment.waiting"];
      const current = await settle(ctx, opts, found.ref, WAITING, events);
      if (SUCCEEDED.includes(current.state)) return ctx.pass({ ...data, state: current.state });
      if (FINISHED.includes(current.state)) {
        return ctx.fail({ ...data, state: current.state, summary: `A release terminou como ${current.state}.` });
      }
      const { decision, pending } = await gate(ctx, opts, found.ref);
      if (decision === "reject") {
        const reason = `o deploy em ${environments(pending)} foi rejeitado`;
        await ctx.impediment(true, reason, effects(opts));
        return ctx.fail({ ...data, state: "rejected", rejected: true, error: reason });
      }
      if (decision === null) await ctx.sleep(opts.every ?? 30000);
    }
  },
});
