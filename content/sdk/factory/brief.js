import { step, actions, markdown } from "sdk";
import { object } from "sdk/steps/context.js";

function line(finding) {
  const found = object(finding);
  const detail = object(found.detail);
  const data = object(detail.data);
  const cause = data.error ?? data.command ?? data.verdict ?? data.body ?? "";
  const gate = found.gate ? ` · gate ${found.gate} → caught by ${found.caught_by}` : "";
  return `- \`${found.id}\` (${found.kind})${gate}${cause ? `: ${String(cause).slice(0, 400)}` : ""}`;
}

function evidence(findings) {
  return findings
    .map((finding) => "```json\n" + JSON.stringify(object(finding).detail ?? {}, null, 2).slice(0, 4000) + "\n```")
    .join("\n\n");
}

export function briefOf(item) {
  const factory = object(item.factory);
  const data = object(factory.data);
  const findings = Array.isArray(data.findings) ? data.findings : [];
  const dir = factory.dir ?? "";
  const escape = factory.kind === "factory.gate.escaped";
  const title = item.title || (escape ? "Review eval" : "Adjust the factory");
  const goal = escape
    ? `Gate \`${data.gate}\` of workflow \`${data.workflow}\` approved work from \`${data.step}\` that was later sent back by \`${data.caught_by}\`. Write an eval case in \`${dir}/evals/\` that reproduces this defect: a command that fails on the defective commit and passes on the current code.`
    : `Step \`${data.step}\` of workflow \`${data.workflow}\` failed ${data.count} times. Adjust the factory in \`${dir}/\` (agents, skills, prompts, steps or the workflow) so it stops failing this way, without loosening any gate.`;
  const text = [
    `# ${title}`,
    "",
    "## Description",
    goal,
    "",
    "## Findings",
    findings.map(line).join("\n") || "- none",
    "",
    "## Evidence",
    evidence(findings),
    "",
    "## Acceptance criteria",
    escape
      ? "- Given the new case, when it runs on the defective commit, then the command fails.\n- Given the new case, when it runs on the current code, then the command passes.\n- The case carries `origin` with the finding's id."
      : "- Given the eval set in `evals/`, when it runs, then every case stays valid.\n- The change touches only factory files.",
  ].join("\n");
  return { title, plan: text, story: goal };
}

export default step({
  name: "brief",
  label: "Briefing",
  icon: "sPlan",
  doing: "Building the briefing",
  view: (s) => [markdown(s.plan ?? ""), ...(s.state === "waiting_human" ? [actions(["approve", "reject"])] : [])],
  async run(ctx, opts) {
    const made = briefOf(ctx.item);
    const autonomy = object(ctx.item.factory).autonomy ?? "near";
    if ((opts.autonomy ?? autonomy) === "in") {
      const answer = await ctx.waitHuman([markdown(made.plan), actions(["approve", "reject"])]);
      if (answer.action !== "approve") return ctx.cancel("the factory briefing was rejected");
    }
    return ctx.pass(made);
  },
});
