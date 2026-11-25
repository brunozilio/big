import { step, markdown, qualityReport } from "sdk";
import { boundPullRequests, defined, eachOf, languageOf, object } from "./context.js";

const HOUR = 3600000;

const TEXTS = {
  en: {
    none: "No repository changed here has a quality project chosen.",
    late: "The quality analysis of the pull request did not arrive in time. Check the analysis in the pipeline and run the step again.",
    failed: "The quality gate failed on the pull request. Fix what it points out in the new code:",
  },
  es: {
    none: "Ningún repositorio cambiado aquí tiene un proyecto de calidad elegido.",
    late: "El análisis de calidad del pull request no llegó a tiempo. Revisa el análisis en el pipeline y vuelve a ejecutar el paso.",
    failed: "El quality gate falló en el pull request. Corrige lo que señala en el código nuevo:",
  },
  "pt-BR": {
    none: "Nenhum repositório alterado aqui tem projeto de qualidade escolhido.",
    late: "A análise de qualidade do pull request não chegou a tempo. Confira a análise no pipeline e rode o passo de novo.",
    failed: "O quality gate falhou no pull request. Corrija o que ele aponta no código novo:",
  },
};

function list(value) {
  return Array.isArray(value) ? value.map(object) : [];
}

const count = (value) => (typeof value === "number" && Number.isFinite(value) ? value : 0);
const percent = (value) => (typeof value === "number" && Number.isFinite(value) ? value : null);
const grade = (value) => (["A", "B", "C", "D", "E"].includes(value) ? value : "A");

export function reportOf(found) {
  const raw = object(found);
  const ratings = object(raw.ratings);
  const counts = object(raw.counts);
  return {
    gate: raw.gate === "failed" ? "failed" : "passed",
    ratings: { reliability: grade(ratings.reliability), security: grade(ratings.security), maintainability: grade(ratings.maintainability) },
    counts: {
      bugs: count(counts.bugs),
      vulnerabilities: count(counts.vulnerabilities),
      smells: count(counts.smells),
      hotspots: count(counts.hotspots),
      hotspotsReviewed: count(counts.hotspotsReviewed),
    },
    coverage: percent(raw.coverage),
    duplication: percent(raw.duplication),
    conditions: list(raw.conditions),
    issues: list(raw.issues),
    total: count(raw.total),
  };
}

async function analyze(ctx, opts, target) {
  const args = defined({ project: target.bound.value, pullRequest: target.number, commit: target.commit });
  const call = { capability: "quality", action: "report", args, connection: target.bound.connection };
  const base = { repository: target.repository, name: target.name, connection: target.bound.connection, project: target.bound.value, pr: target.pr, branch: target.head ?? "" };
  try {
    const found = object(await ctx.poll(call, opts.every ?? 30000, defined({ until: { status: ["done"] }, timeout: opts.timeout ?? HOUR })));
    return { ...base, url: found.url ?? null, analyzedAt: found.analyzedAt ?? null, report: reportOf(found) };
  } catch {
    return { ...base, url: null, analyzedAt: null, report: null };
  }
}

function issueLine(issue) {
  const where = issue.file ? ` (${issue.file}${issue.line ? `:${issue.line}` : ""})` : "";
  return `- ${issue.type} ${issue.severity}: ${issue.message}${where}`;
}

function conditionLine(condition) {
  return `- ${condition.metric}: ${condition.value} (limit ${condition.threshold})`;
}

function explained(failed) {
  return failed
    .map((entry) => {
      const conditions = entry.report.conditions.filter((condition) => condition.passed === false).map(conditionLine);
      const issues = entry.report.issues.map(issueLine);
      return [failed.length > 1 && entry.name ? `## ${entry.name}` : "", ...conditions, ...issues].filter(Boolean).join("\n");
    })
    .join("\n\n");
}

function view(s) {
  const reports = list(s.reports).filter((entry) => entry.report);
  if (reports.length === 0) return s.reason ? [markdown(s.reason)] : [];
  return reports.map((entry) => qualityReport({ connection: entry.connection, project: entry.project, branch: entry.branch, analyzedAt: entry.analyzedAt, url: entry.url, report: entry.report }));
}

export default step({
  name: "quality",
  label: "Quality",
  icon: "check",
  requires: "quality",
  doing: "Waiting for the quality analysis",
  view,
  async run(ctx, opts) {
    const texts = TEXTS[languageOf(ctx)] ?? TEXTS.en;
    const targets = boundPullRequests(ctx, opts, "quality");
    if (targets.length === 0) return ctx.skip({ reason: texts.none });
    const reports = await eachOf(ctx, targets, (branch, target) => analyze(branch, opts, target));
    if (reports.some((entry) => !entry.report)) return ctx.fail({ reports, error: texts.late });
    const failed = reports.filter((entry) => entry.report.gate === "failed");
    if (failed.length === 0) return ctx.pass({ reports });
    const target = opts.onFail === undefined ? "develop" : opts.onFail;
    const rounds = opts.retries ?? 3;
    const body = explained(failed);
    if (target && ctx.attempt <= rounds) return ctx.goto(target, { reports, summary: texts.failed, body });
    return ctx.fail({ reports, error: `${texts.failed}\n${body}` });
  },
});
