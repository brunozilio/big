import { step, actions, markdown } from "sdk";
import { object } from "sdk/steps/context.js";

function line(finding) {
  const found = object(finding);
  const detail = object(found.detail);
  const data = object(detail.data);
  const cause = data.error ?? data.command ?? data.verdict ?? data.body ?? "";
  const gate = found.gate ? ` · portão ${found.gate} → pego por ${found.caught_by}` : "";
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
  const title = item.title || (escape ? "Revisar avaliação" : "Ajustar a fábrica");
  const goal = escape
    ? `O portão \`${data.gate}\` do workflow \`${data.workflow}\` aprovou um trabalho de \`${data.step}\` que depois foi devolvido por \`${data.caught_by}\`. Escreva um caso de avaliação em \`${dir}/evals/\` que reproduza esse defeito: um comando que falha no commit defeituoso e passa no código atual.`
    : `A etapa \`${data.step}\` do workflow \`${data.workflow}\` falhou ${data.count} vezes. Ajuste a fábrica em \`${dir}/\` (agentes, skills, prompts, etapas ou o workflow) para que ela pare de falhar assim, sem afrouxar nenhum portão.`;
  const text = [
    `# ${title}`,
    "",
    "## Descrição",
    goal,
    "",
    "## Achados",
    findings.map(line).join("\n") || "- nenhum",
    "",
    "## Evidência",
    evidence(findings),
    "",
    "## Critérios de aceite",
    escape
      ? "- Dado o novo caso, quando ele roda no commit defeituoso, então o comando falha.\n- Dado o novo caso, quando ele roda no código atual, então o comando passa.\n- O caso traz `origin` com o id do achado."
      : "- Dado o conjunto de avaliações em `evals/`, quando roda, então todos os casos continuam válidos.\n- A mudança fica só em arquivos da fábrica.",
  ].join("\n");
  return { title, plan: text, story: goal };
}

export default step({
  name: "brief",
  label: "Briefing",
  icon: "sPlan",
  doing: "Montando o briefing",
  view: (s) => [markdown(s.plan ?? ""), ...(s.state === "waiting_human" ? [actions(["approve", "reject"])] : [])],
  async run(ctx, opts) {
    const made = briefOf(ctx.item);
    const autonomy = object(ctx.item.factory).autonomy ?? "near";
    if ((opts.autonomy ?? autonomy) === "in") {
      const answer = await ctx.waitHuman([markdown(made.plan), actions(["approve", "reject"])]);
      if (answer.action !== "approve") return ctx.cancel("o briefing da fábrica foi recusado");
    }
    return ctx.pass(made);
  },
});
