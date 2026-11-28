import { step, markdown, findingList } from "sdk";
import { projectOf } from "sdk/bindings.js";
import { boundPullRequests, codeOptions, defined, eachOf, effects, languageOf, object, waitOptions } from "sdk/steps/context.js";

const MINUTE = 60000;
const DAY = 24 * 60 * MINUTE;

export const TRIAGE = "findings.triage";

const CHOICES = ["fix", "task", "ignore"];

const TEXTS = {
  en: {
    none: "No repository changed here has a security project chosen.",
    clean: "No open security finding in the files this change touched.",
    fix: "The person reviewing security asked to fix these findings in the files this change touched, at their root cause:",
    task: "Security finding",
    late: "The security analysis after the merge did not arrive in time. Run the step again when the scanner finishes.",
    title: "Fix security findings",
  },
  es: {
    none: "Ningún repositorio cambiado aquí tiene un proyecto de seguridad elegido.",
    clean: "Ninguna vulnerabilidad abierta en los archivos que tocó este cambio.",
    fix: "Quien revisa la seguridad pidió corregir estas vulnerabilidades en los archivos que tocó este cambio, en su causa raíz:",
    task: "Vulnerabilidad",
    late: "El análisis de seguridad después del merge no llegó a tiempo. Vuelve a ejecutar el paso cuando el escáner termine.",
    title: "Corregir vulnerabilidades",
  },
  "pt-BR": {
    none: "Nenhum repositório alterado aqui tem projeto de segurança escolhido.",
    clean: "Nenhuma vulnerabilidade aberta nos arquivos que esta mudança tocou.",
    fix: "Quem revisa a segurança pediu para corrigir estas vulnerabilidades nos arquivos que esta mudança tocou, na causa raiz:",
    task: "Vulnerabilidade",
    late: "A análise de segurança depois do merge não chegou a tempo. Rode o passo de novo quando o scanner terminar.",
    title: "Corrigir vulnerabilidades",
  },
};

function list(value) {
  return Array.isArray(value) ? value.map(object) : [];
}

export function touches(paths, file) {
  if (!file) return false;
  return paths.some((path) => path === file || path.endsWith(`/${file}`) || file.endsWith(`/${path}`));
}

export function suggestedOf(finding) {
  return finding.policy === true || Number(finding.severity) >= 4 ? "fix" : "task";
}

function shown(finding, decision) {
  return {
    id: String(finding.id),
    title: String(finding.title ?? ""),
    severity: Number(finding.severity) || 0,
    kind: finding.kind ?? "static",
    new: finding.new === true,
    policy: finding.policy === true,
    cwe: finding.cwe ?? null,
    cve: finding.cve ?? null,
    file: finding.file ?? null,
    line: finding.line ?? null,
    component: finding.component ?? null,
    version: finding.version ?? null,
    fixedIn: finding.fixedIn ?? null,
    description: String(finding.description ?? ""),
    held: null,
    suggested: suggestedOf(finding),
    decision: decision ?? null,
    url: finding.url ?? null,
  };
}

async function analyzed(ctx, opts, target) {
  if (opts.wait === false) return true;
  const options = effects({ ...opts, connection: target.bound.connection });
  const before = object(await ctx.call("security", "status", { project: target.bound.value }, options));
  const args = defined({ project: target.bound.value, since: before.scannedAt ?? undefined });
  const call = { capability: "security", action: "status", args, connection: target.bound.connection };
  try {
    await ctx.poll(call, opts.every ?? 15 * MINUTE, defined({ until: { status: ["done"] }, timeout: opts.timeout ?? 3 * DAY }));
    return true;
  } catch {
    return false;
  }
}

async function scan(ctx, opts, target, known) {
  if (!(await analyzed(ctx, opts, target))) return { repository: target.repository, late: true, findings: [] };
  const files = await ctx.call("code", "files_diff", { pr: target.pr }, effects(codeOptions(opts, target)));
  const paths = list(files).map((file) => String(file.path ?? "")).filter(Boolean);
  const args = defined({ project: target.bound.value, open: true, minSeverity: opts.minSeverity, policy: opts.policy, limit: opts.scan ?? 500 });
  const found = list(await ctx.call("security", "findings", args, effects({ ...opts, connection: target.bound.connection })));
  const relevant = opts.scope === "all" ? found : found.filter((finding) => touches(paths, finding.file));
  const findings = relevant.filter((finding) => finding.id !== undefined).map((finding) => shown(finding, known.get(String(finding.id))));
  return { repository: target.repository, name: target.name, connection: target.bound.connection, project: target.bound.value, pr: target.pr, findings };
}

export function decisionOf(given, suggested) {
  const chosen = object(given);
  const choice = CHOICES.includes(chosen.choice) ? chosen.choice : suggested;
  const reason = choice === "ignore" ? (chosen.reason === "false_positive" ? "false_positive" : "accepted_risk") : null;
  return { choice, reason, task: null, pr: null };
}

function line(finding) {
  const where = finding.file ? ` (${finding.file}${finding.line ? `:${finding.line}` : ""})` : finding.component ? ` (${finding.component})` : "";
  const cwe = finding.cwe && finding.cwe.id ? ` CWE-${finding.cwe.id}` : "";
  return `- #${finding.id} ${finding.title}${where}, severity ${finding.severity}${cwe}${finding.description ? `\n  ${finding.description}` : ""}`;
}

async function opened(ctx, opts, finding, texts) {
  const tracker = projectOf(ctx.item.application, "tracker");
  const args = defined({ title: `${texts.task}: ${finding.title}`, description: line(finding), project: tracker.project });
  const made = await ctx.call("tracker", "create_issue", args, { ...effects({ ...opts, connection: tracker.connection }), optional: true });
  return made && made.ref ? { ref: made.ref, url: made.url ?? null } : null;
}

function previousOf(ctx) {
  const known = new Map();
  for (const scanned of list(object(ctx.output(ctx.step)).scans)) {
    for (const finding of list(scanned.findings)) if (finding.decision) known.set(String(finding.id), finding.decision);
  }
  return known;
}

function view(s) {
  const scans = list(s.scans).filter((scanned) => list(scanned.findings).length > 0);
  if (scans.length === 0) return s.reason ? [markdown(s.reason)] : [];
  return scans.map((scanned) => findingList({ connection: scanned.connection, project: scanned.project, url: null, scannedAt: null, triage: false, findings: scanned.findings }));
}

async function decide(ctx, opts, scans, texts) {
  const asking = scans.filter((scanned) => scanned.findings.some((finding) => !finding.decision));
  if (asking.length === 0) return scans;
  const blocks = asking.map((scanned) => findingList({ connection: scanned.connection, project: scanned.project, url: null, scannedAt: null, triage: true, findings: scanned.findings }));
  const answer = await ctx.waitHuman(blocks, waitOptions(opts));
  const given = object(object(answer.values).decisions);
  const decided = [];
  for (const scanned of scans) {
    const findings = [];
    for (const finding of scanned.findings) {
      const decision = finding.decision ?? decisionOf(given[finding.id], finding.suggested);
      if (!finding.decision && decision.choice === "task") decision.task = await opened(ctx, opts, finding, texts);
      findings.push({ ...finding, decision });
    }
    decided.push({ ...scanned, findings });
  }
  return decided;
}

async function fixed(ctx, opts, scans, fixing, texts) {
  const workflow = opts.fixWith === undefined ? "security" : opts.fixWith;
  if (!workflow) return [];
  const application = object(ctx.item.application);
  const repositories = Array.isArray(application.repositories) ? application.repositories : [];
  const children = [];
  for (const scanned of scans) {
    const findings = fixing.filter((finding) => scanned.findings.some((known) => known.id === finding.id));
    if (findings.length === 0) continue;
    const kept = repositories.filter((repository) => object(repository).id === scanned.repository);
    const title = `${texts.title} · ${scanned.name ?? scanned.project}`;
    const input = { title, application: { ...application, repositories: kept }, findings, language: ctx.item.language };
    children.push(object(await ctx.child(workflow, input)));
  }
  return children;
}

export default step({
  name: "security",
  label: "Security",
  icon: "shield",
  requires: "security",
  doing: "Checking security",
  view,
  async run(ctx, opts) {
    const texts = TEXTS[languageOf(ctx)] ?? TEXTS.en;
    const targets = boundPullRequests(ctx, opts, "security");
    if (targets.length === 0) return ctx.pass({ reason: texts.none });
    const known = previousOf(ctx);
    const results = await eachOf(ctx, targets, (branch, target) => scan(branch, opts, target, known));
    if (results.some((entry) => entry.late)) return ctx.fail({ scans: results.filter((entry) => !entry.late), error: texts.late });
    if (results.every((entry) => entry.findings.length === 0)) return ctx.pass({ scans: results, reason: texts.clean });
    const fresh = results.flatMap((entry) => entry.findings).filter((finding) => !finding.decision).map((finding) => finding.id);
    const scans = await decide(ctx, opts, results, texts);
    const fixing = scans.flatMap((entry) => entry.findings).filter((finding) => fresh.includes(finding.id) && finding.decision.choice === "fix");
    if (fixing.length === 0) return ctx.pass({ scans });
    if (opts.onFix) return ctx.goto(opts.onFix, { scans, summary: texts.fix, body: fixing.map(line).join("\n") });
    const children = await fixed(ctx, opts, scans, fixing, texts);
    return ctx.pass(defined({ scans, fixes: children.length > 0 ? children : undefined }));
  },
});
