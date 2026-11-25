import { step, actions, chat, form, markdown, pipeline } from "sdk";
import { inLanguage, FINISHED, SUCCEEDED, agentOptions, defined, eachOf, effects, findRun, object, pipelineOptions, pullRequestsOf, repoOf, settle, source, targetFields, waitOptions } from "./context.js";

const WAITING = [...FINISHED, "waiting"];

const environments = (pending) => pending.map((deployment) => deployment.environment).join(", ");

async function decide(ctx, opts, run, pending) {
  if (opts.approve === "auto") return "approve";
  const view = [
    pipeline(run),
    markdown(`Deploy awaiting approval: ${environments(pending)}`),
    actions(["approve_deploy", "reject_deploy"]),
  ];
  const raced = await ctx.race([
    (branch) => branch.waitHuman(view, waitOptions(opts)),
    (branch) => settle(branch, opts, run, FINISHED, ["pipelines.run.finished"]),
  ]);
  if (raced.index === 1) return "outside";
  return raced.value.action === "approve_deploy" ? "approve" : "reject";
}

const REASON = [{ name: "reason", label: "Reason", kind: "textarea", placeholder: "What blocks this deploy" }];

async function rejectionReason(ctx, opts, pending) {
  if (opts.approve === "auto") return null;
  const view = [
    markdown(`You rejected the deploy to ${environments(pending)}. Give the reason to record on the release and the item.`),
    form(REASON, { title: "Rejection reason", submit: "Reject deploy" }),
  ];
  for (;;) {
    const answer = await ctx.waitHuman(view, waitOptions(opts));
    const reason = String(object(answer.values).reason ?? "").trim();
    if (reason) return reason;
  }
}

async function gate(ctx, opts, run) {
  const listed = await ctx.call("pipelines", "pending_deployments", { run }, effects(opts));
  const pending = (Array.isArray(listed) ? listed : []).filter((deployment) => deployment.canApprove !== false);
  if (pending.length === 0) return { decision: null, pending };
  const decision = await decide(ctx, opts, run, pending);
  if (decision === "outside") return { decision, pending };
  const deployments = pending.map((deployment) => deployment.id);
  const why = decision === "reject" ? await rejectionReason(ctx, opts, pending) : null;
  const args = defined({ run, deployments, decision, comment: why ?? undefined });
  await ctx.call("pipelines", "review_deployment", args, effects(opts));
  return { decision, pending, why };
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
  const found = await findRun(ctx, options, { ref: target.ref, repo: repoOf(target), seen, optional: true });
  if (!found) return { ...targetFields(target), run: null, url: null, state: NONE, ref: target.ref };
  const data = { ...targetFields(target), run: found.ref, url: found.url ?? null };
  for (;;) {
    const events = ["pipelines.run.finished", "pipelines.deployment.waiting"];
    const current = await settle(ctx, options, found.ref, WAITING, events);
    if (SUCCEEDED.includes(current.state)) return { ...data, state: current.state };
    if (FINISHED.includes(current.state)) return { ...data, state: current.state, summary: `The release finished as ${current.state}.` };
    const { decision, pending, why } = await gate(ctx, options, found.ref);
    if (decision === "reject") {
      const reason = `the deploy to ${environments(pending)} was rejected${why ? `: ${why}` : ""}`;
      await ctx.impediment(true, reason, effects(options));
      return { ...data, state: "rejected", rejected: true, error: reason, ...(why ? { why } : {}) };
    }
    if (decision === null) await ctx.sleep(opts.every ?? 30000);
  }
}

function described(runs) {
  return runs
    .map((run) => `- ${run.name ? `${run.name}: ` : ""}run ${run.run} finished as ${run.state}${run.error ? ` (${run.error})` : ""}`)
    .join("\n");
}

async function narrate(ctx, opts, runs, failed) {
  const prs = pullRequestsOf(ctx, opts).map((entry) => entry.pr).filter(Boolean);
  const merged = source(ctx, opts, "merge");
  const ask = failed
    ? "The release failed. Read the pipeline log, state the likely cause and whether the failure comes from the change or the environment."
    : "The release finished. Write the release notes for the product's users.";
  const prompt = [
    ask,
    prs.length > 0 ? `Pull requests: ${prs.join(", ")}` : null,
    merged.sha ? `Published commit: ${merged.sha}` : null,
    `Pipelines:\n${described(runs)}`,
  ].filter(Boolean).join("\n\n");
  const input = defined({ prompt: inLanguage(ctx, prompt), prs, runs: runs.map((run) => run.run), failed: Boolean(failed) });
  const result = await ctx.agent(opts.agent, input, agentOptions(opts, { mode: opts.mode ?? "plan" }));
  const output = object(result.output);
  return defined({ session: result.session, notes: output.notes ?? output.summary ?? result.text ?? null });
}

const NONE = "none";

function shown(s) {
  const listed = Array.isArray(s.runs) && s.runs.length > 0 ? s.runs : s.run ? [s] : [];
  const none = listed.length > 0 ? listed.every((run) => run.state === NONE) : Boolean(s.ref);
  if (none) return [markdown("No release pipeline configured.")];
  const runs = listed.map((run) => pipeline(run.run));
  const rejected = [s, ...listed].find((run) => run.why);
  const why = rejected ? [markdown(`**Deploy rejected.** ${rejected.why}`)] : [];
  return [...runs, ...why, ...(s.notes ? [markdown(s.notes)] : []), ...(s.session ? [chat(s.session)] : [])];
}

export default step({
  name: "release",
  label: "Release",
  icon: "sRelease",
  requires: "pipelines",
  doing: "Deploy",
  view: shown,
  async run(ctx, opts) {
    const targets = targetsOf(ctx, opts);
    if (targets.some((target) => !target.ref)) return ctx.fail({ error: "there is no commit or branch to follow the release" });
    const seen = object(ctx.output(ctx.step)).seen ?? [];
    const runs = await eachOf(ctx, targets, (branch, target) => follow(branch, opts, target, seen));
    if (runs.every((run) => run.state === NONE)) return ctx.skip({ state: NONE, ref: runs[0].ref, runs });
    const first = runs[0];
    const data = { run: first.run, url: first.url, seen: [...seen, ...runs.filter((run) => run.run).map((run) => run.run)] };
    const listed = runs.length > 1 || first.repository ? { ...data, runs } : data;
    const failed = runs.find((run) => !SUCCEEDED.includes(run.state));
    const told = opts.agent && !(failed && failed.rejected) ? await narrate(ctx, opts, runs, failed) : {};
    if (!failed) return ctx.pass({ ...listed, ...told, state: first.state });
    const { run: _run, url: _url, repository: _repository, name: _name, code: _code, ...why } = failed;
    return ctx.fail({ ...listed, ...told, ...why });
  },
});
