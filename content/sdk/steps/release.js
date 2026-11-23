import { step, actions, chat, markdown, pipeline } from "sdk";
import { inLanguage, FINISHED, SUCCEEDED, agentOptions, defined, eachOf, effects, findRun, object, pipelineOptions, pullRequestsOf, repoOf, settle, source, targetFields, waitOptions } from "./context.js";

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

function targetsOf(ctx, opts) {
  const merged = source(ctx, opts, "merge");
  const prs = pullRequestsOf(ctx, opts);
  const merges = Array.isArray(merged.merges) && merged.merges.length > 0 ? merged.merges : null;
  if (opts.ref || !merges) return [{ ...(prs[0] ?? {}), ref: opts.ref ?? merged.sha ?? prs[0]?.base }];
  return merges.map((entry) => {
    const pr = prs.find((found) => found.pr === entry.pr) ?? {};
    return { ...pr, ...entry, ref: entry.sha ?? pr.base };
  });
}

async function follow(ctx, opts, target, seen) {
  const options = pipelineOptions(opts, target);
  const found = await findRun(ctx, options, { ref: target.ref, repo: repoOf(target), seen });
  const data = { ...targetFields(target), run: found.ref, url: found.url ?? null };
  for (;;) {
    const events = ["pipelines.run.finished", "pipelines.deployment.waiting"];
    const current = await settle(ctx, options, found.ref, WAITING, events);
    if (SUCCEEDED.includes(current.state)) return { ...data, state: current.state };
    if (FINISHED.includes(current.state)) return { ...data, state: current.state, summary: `A release terminou como ${current.state}.` };
    const { decision, pending } = await gate(ctx, options, found.ref);
    if (decision === "reject") {
      const reason = `o deploy em ${environments(pending)} foi rejeitado`;
      await ctx.impediment(true, reason, effects(options));
      return { ...data, state: "rejected", rejected: true, error: reason };
    }
    if (decision === null) await ctx.sleep(opts.every ?? 30000);
  }
}

function described(runs) {
  return runs
    .map((run) => `- ${run.name ? `${run.name}: ` : ""}execução ${run.run} terminou como ${run.state}${run.error ? ` (${run.error})` : ""}`)
    .join("\n");
}

async function narrate(ctx, opts, runs, failed) {
  const prs = pullRequestsOf(ctx, opts).map((entry) => entry.pr).filter(Boolean);
  const merged = source(ctx, opts, "merge");
  const ask = failed
    ? "A release falhou. Leia o log do pipeline, diga a causa provável e se a falha é da mudança ou do ambiente."
    : "A release terminou. Escreva as notas da release para quem usa o produto.";
  const prompt = [
    ask,
    prs.length > 0 ? `Pull requests: ${prs.join(", ")}` : null,
    merged.sha ? `Commit publicado: ${merged.sha}` : null,
    `Pipelines:\n${described(runs)}`,
  ].filter(Boolean).join("\n\n");
  const input = defined({ prompt: inLanguage(prompt), prs, runs: runs.map((run) => run.run), failed: Boolean(failed) });
  const result = await ctx.agent(opts.agent, input, agentOptions(opts, { mode: opts.mode ?? "plan" }));
  const output = object(result.output);
  return defined({ session: result.session, notes: output.notes ?? output.summary ?? result.text ?? null });
}

function shown(s) {
  const runs = (Array.isArray(s.runs) && s.runs.length > 0 ? s.runs : s.run ? [s] : []).map((run) => pipeline(run.run));
  return [...runs, ...(s.notes ? [markdown(s.notes)] : []), ...(s.session ? [chat(s.session)] : [])];
}

export default step({
  name: "release",
  label: "Release",
  icon: "sRelease",
  needs: "pipelines",
  doing: "Deploy",
  view: shown,
  async run(ctx, opts) {
    const targets = targetsOf(ctx, opts);
    if (targets.some((target) => !target.ref)) return ctx.fail({ error: "não há commit nem branch para acompanhar a release" });
    const seen = object(ctx.output(ctx.step)).seen ?? [];
    const runs = await eachOf(ctx, targets, (branch, target) => follow(branch, opts, target, seen));
    const first = runs[0];
    const data = { run: first.run, url: first.url, seen: [...seen, ...runs.map((run) => run.run)] };
    const listed = runs.length > 1 || first.repository ? { ...data, runs } : data;
    const failed = runs.find((run) => !SUCCEEDED.includes(run.state));
    const told = opts.agent && !(failed && failed.rejected) ? await narrate(ctx, opts, runs, failed) : {};
    if (!failed) return ctx.pass({ ...listed, ...told, state: first.state });
    const { run: _run, url: _url, repository: _repository, name: _name, code: _code, ...why } = failed;
    return ctx.fail({ ...listed, ...told, ...why });
  },
});
