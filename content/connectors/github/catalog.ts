import { type Language, pick } from "sdk/connectors/index.ts";
import { MAYBE_TEXT, NUMBER, TEXT, event } from "sdk/connectors/events.ts";

export const REVIEWED = "github.review.submitted";
export const MERGED = "github.pr.merged";
export const CLOSED = "github.pr.closed";
export const UPDATED = "github.pr.updated";
export const FINISHED = "github.run.finished";
export const WAITING = "github.deployment.waiting";

const PULL = { pr: TEXT, repo: TEXT, number: NUMBER, head: TEXT, base: TEXT, title: MAYBE_TEXT };
const PULL_REQUIRED = ["pr", "repo", "number", "head", "base"];
const RUN = { run: TEXT, repo: TEXT, pipeline: MAYBE_TEXT, head: MAYBE_TEXT, sha: MAYBE_TEXT, state: TEXT };

function pull(language: Language, name: string, label: [string, string, string], alias: string) {
  return event(name, {
    label: pick(language, ...label),
    capability: "code",
    source: "pull_request",
    aliases: [alias],
    properties: { ...PULL, state: TEXT },
    required: [...PULL_REQUIRED, "state"],
    filters: ["repo", "base", "head"],
  });
}

function run(language: Language, name: string, label: [string, string, string], alias: string) {
  return event(name, {
    label: pick(language, ...label),
    capability: "pipelines",
    aliases: [alias],
    properties: RUN,
    required: ["run", "repo", "state"],
    filters: ["repo", "pipeline", "head", "state"],
  });
}

export function events({ language }: { language: Language }) {
  return [
    event(REVIEWED, {
      label: pick(language, "Review submitted", "Revisión enviada", "Revisão enviada"),
      capability: "code",
      source: "pull_request",
      aliases: ["code.review.submitted"],
      properties: { ...PULL, verdict: TEXT, author: TEXT, body: TEXT },
      required: [...PULL_REQUIRED, "verdict"],
      filters: ["verdict", "repo", "base", "author"],
    }),
    pull(language, MERGED, ["Pull request merged", "Pull request fusionado", "Pull request integrado"], "code.pr.merged"),
    pull(language, CLOSED, ["Pull request closed", "Pull request cerrado", "Pull request fechado"], "code.pr.closed"),
    pull(language, UPDATED, ["Pull request updated", "Pull request actualizado", "Pull request atualizado"], "code.pr.updated"),
    run(language, FINISHED, ["Pipeline run finished", "Ejecución terminada", "Execução terminada"], "pipelines.run.finished"),
    run(language, WAITING, ["Deployment waiting", "Despliegue en espera", "Deploy aguardando"], "pipelines.deployment.waiting"),
  ];
}

export function legacyKind(kind: string): string {
  const found = events({ language: "en" }).find((spec) => spec.name === kind);
  return found?.aliases[0] ?? kind;
}
