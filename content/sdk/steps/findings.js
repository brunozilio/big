import { projectOf } from "sdk/bindings.js";
import { step, markdown } from "sdk";
import { attempt, codeTargets, defined, effects, languageOf, object, parsePlan, repoOf, repositoriesOf } from "./context.js";

const MARK = "security-findings";

const SEVERITIES = ["Informational", "Very low", "Low", "Medium", "High", "Very high"];

const TEXTS = {
  en: { title: "Fix security findings", none: "No new security findings to fix.", unset: "No repository of the application has a security project chosen. Choose one in Settings → Applications." },
  es: { title: "Corregir vulnerabilidades", none: "No hay vulnerabilidades nuevas para corregir.", unset: "Ningún repositorio de la aplicación tiene un proyecto de seguridad elegido. Elige uno en Ajustes → Aplicaciones." },
  "pt-BR": { title: "Corrigir vulnerabilidades", none: "Nenhuma vulnerabilidade nova para corrigir.", unset: "Nenhum repositório da aplicação tem projeto de segurança escolhido. Escolha um em Configurações → Aplicações." },
};

export function markerOf(ids) {
  return `<!-- ${MARK}: ${ids.join(",")} -->`;
}

export function heldIn(body) {
  const found = new RegExp(`<!--\\s*${MARK}:\\s*([^>]*?)\\s*-->`).exec(String(body ?? ""));
  return found ? found[1].split(",").map((id) => id.trim()).filter(Boolean) : [];
}

export function severityOf(level) {
  const at = Math.max(0, Math.min(SEVERITIES.length - 1, Math.round(Number(level) || 0)));
  return SEVERITIES[at];
}

function list(value) {
  return Array.isArray(value) ? value.map(object) : [];
}

function securityOf(ctx, opts) {
  const holder = repositoriesOf(ctx).find((known) => projectOf(known, "security").project) ?? ctx.item.application;
  const chosen = projectOf(holder, "security");
  return { project: opts.project ?? chosen.project, connection: opts.connection ?? chosen.connection };
}

async function held(ctx, opts) {
  const ids = new Set();
  if (opts.skipHeld === false) return ids;
  for (const target of codeTargets(ctx, {})) {
    const repo = repoOf(target);
    if (!repo) continue;
    const options = effects({ connection: object(target.code).connection, timeout: opts.timeout, retry: opts.retry });
    const open = await attempt(() => ctx.call("code", "pull_requests", { repo }, options));
    for (const pr of list(open)) heldIn(pr.body).forEach((id) => ids.add(id));
  }
  return ids;
}

function where(finding) {
  if (finding.file) return finding.line ? `${finding.file}:${finding.line}` : finding.file;
  if (finding.component) return finding.version ? `${finding.component} ${finding.version}` : finding.component;
  return finding.module ?? "";
}

function named(finding) {
  const cwe = object(finding.cwe);
  const cve = object(finding.cve);
  const kind = cve.id ? cve.id : cwe.id ? `CWE-${cwe.id}` : finding.kind ?? "";
  const title = finding.title || cwe.name || cve.id || `#${finding.id}`;
  return kind && !title.includes(kind) ? `${title} (${kind})` : title;
}

function details(finding) {
  const cve = object(finding.cve);
  const lines = [
    `- Id: ${finding.id}`,
    `- Severity: ${severityOf(finding.severity)} (${finding.severity})`,
    `- Kind: ${finding.kind ?? "unknown"}`,
    where(finding) ? `- Where: \`${where(finding)}\`` : "",
    cve.cvss !== undefined && cve.cvss !== null ? `- CVSS: ${cve.cvss}` : "",
    finding.policy ? "- Violates the security policy" : "",
    finding.url ? `- Link: ${finding.url}` : "",
  ].filter(Boolean);
  const text = String(finding.description ?? "").trim();
  return text ? `${lines.join("\n")}\n\n${text}` : lines.join("\n");
}

function guidance(opts) {
  return [
    "Fix each finding below at its root cause, in the application's code or dependencies, with the smallest change that removes the vulnerability.",
    "- For a dependency finding, upgrade to the nearest version that fixes it and adjust any code the upgrade breaks.",
    "- Never silence the scanner: no suppression comments, no ignore lists, no disabled rules.",
    "- When a finding is a false positive or cannot be fixed safely in code, leave it untouched and explain why in the final summary.",
    opts.instructions ?? "",
  ]
    .filter(Boolean)
    .join("\n");
}

export function planText(title, chosen, opts) {
  const criteria = [
    "- Each listed finding is fixed at its root cause or explained as not fixable.",
    "- The repository's tests, lint and build pass.",
    "- Tests cover the fixed behavior whenever the fix changes code paths.",
  ].join("\n");
  const subtasks = chosen.map((finding) => `### ${named(finding)}\n${details(finding)}`).join("\n\n");
  return `# ${title}\n\n## Description\n${guidance(opts)}\n\n## Acceptance criteria\n${criteria}\n\n## Subtasks\n${subtasks}`;
}

function bodyOf(chosen) {
  const rows = chosen.map((finding) => `| ${finding.id} | ${severityOf(finding.severity)} | ${named(finding)} | ${where(finding) ? `\`${where(finding)}\`` : ""} |`);
  const table = ["| Id | Severity | Finding | Where |", "|---|---|---|---|", ...rows].join("\n");
  return `${table}\n\n${markerOf(chosen.map((finding) => String(finding.id)))}`;
}

function dayOf(ctx) {
  const scheduled = object(ctx.item.schedule).day;
  return scheduled ?? new Date(ctx.now()).toISOString().slice(0, 10);
}

function shown(s) {
  const chosen = list(s.findings);
  if (chosen.length === 0) return s.reason ? [markdown(s.reason)] : [];
  return [markdown(bodyOf(chosen).split("\n\n<!--")[0])];
}

export default step({
  name: "findings",
  label: "Findings",
  icon: "shield",
  requires: "security",
  doing: "Reading findings",
  view: shown,
  async run(ctx, opts) {
    const previous = object(ctx.output(ctx.step));
    if (Array.isArray(previous.findings) && previous.findings.length > 0) return ctx.pass(previous);
    const texts = TEXTS[languageOf(ctx)] ?? TEXTS.en;
    const security = securityOf(ctx, opts);
    if (!security.project) return ctx.fail({ error: texts.unset });
    const args = defined({ project: security.project, kinds: opts.kinds, minSeverity: opts.minSeverity ?? 3, policy: opts.policy, open: true, limit: opts.scan ?? 200 });
    const handed = list(ctx.item.findings);
    const found = handed.length > 0 ? handed : list(await ctx.call("security", "findings", args, effects({ ...opts, connection: security.connection })));
    const taken = await held(ctx, opts);
    const fresh = found.filter((finding) => finding.id !== undefined && !taken.has(String(finding.id)));
    const chosen = fresh.slice(0, opts.batch ?? 5);
    if (chosen.length === 0) return ctx.done({ findings: [], reason: texts.none, open: found.length, held: [...taken] });
    const title = `${opts.title ?? texts.title} · ${dayOf(ctx)}`;
    const written = parsePlan(planText(title, chosen, opts), languageOf(ctx));
    return ctx.pass({
      title,
      plan: written.text,
      story: written.story,
      subtasks: opts.subtasks === false ? [] : written.subtasks,
      findings: chosen,
      left: fresh.length - chosen.length,
      held: [...taken],
      pullRequestBody: bodyOf(chosen),
    });
  },
});
