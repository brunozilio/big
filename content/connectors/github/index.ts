import { type Creds, type Language, pick, words, textOf, tokenAlternative, tokenField, unknownAction } from "sdk/connectors/index.ts";
import { binding, code, options, pipelines } from "sdk/connectors/contracts.ts";
import { api, get } from "./api.ts";
import * as reviews from "./code.ts";
import * as runs from "./pipelines.ts";

export { poll } from "./events.ts";

export function provider(_: unknown, ctx) {
  return {
    name: "GitHub",
    logo: "logo.svg",
    capabilities: ["code", "pipelines"],
    auth: {
      kind: "device",
      client_id: ctx.config.client_id,
      device_endpoint: ctx.config.device_endpoint,
      token_endpoint: ctx.config.token_endpoint,
      scopes: words(ctx.config.scopes),
      dialect: "form",
    },
    actions: [...code(), ...pipelines(), options("code")],
  };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(language, "Your GitHub account", "Tu cuenta de GitHub", "Sua conta GitHub"),
    group: "code",
    button: pick(language, "Sign in with GitHub", "Entrar con GitHub", "Entrar com GitHub"),
    site: pick(language, "GitHub", "de GitHub", "do GitHub"),
    choose_title: pick(language, "Organization", "Organización", "Organização"),
    hidden: false,
    bindings: [binding("code", "repository", "GitHub", { section: pick(language, "Code", "Código", "Código"), order: 30 })],
  };
}

export function alternatives({ language }: { language: Language }) {
  const help = pick(
    language,
    "On GitHub, open Settings, Developer settings, Personal access tokens and create a token with access to the repositories, the organization and the workflows.",
    "En GitHub, abre Settings, Developer settings, Personal access tokens y genera un token con acceso a los repositorios, a la organización y a los workflows.",
    "No GitHub, abra Settings, Developer settings, Personal access tokens e gere um token com acesso aos repositórios, à organização e aos workflows.",
  );
  const label = pick(language, "GitHub personal access token", "Personal access token de GitHub", "Personal access token do GitHub");
  const way = pick(language, "Use a personal access token", "Usar un personal access token", "Usar personal access token");
  const site = pick(language, "GitHub", "de GitHub", "do GitHub");
  return [tokenAlternative(language, "api_token", way, site, [tokenField("token", label, help, true)], "bearer")];
}

const ACTIONS: Record<string, (connection: any, args: Record<string, any>, key: string) => unknown> = {
  "code.open_pull_request": reviews.openPullRequest,
  "code.pull_request": reviews.pullRequest,
  "code.files_diff": reviews.filesDiff,
  "code.review_comments": reviews.reviewComments,
  "code.submit_review": reviews.submitReview,
  "code.resolve_thread": reviews.resolveThread,
  "code.merge": reviews.merge,
  "code.repos": reviews.repos,
  "code.options": (connection, args) => reviews.repos(connection, args).map((repo: any) => ({ value: repo.ref, label: repo.ref })),
  "code.pull_requests": reviews.pullRequests,
  "code.branches": reviews.branches,
  "code.merge_branches": reviews.mergeBranches,
  "pipelines.runs": runs.runs,
  "pipelines.run": runs.run,
  "pipelines.job_log": runs.jobLog,
  "pipelines.pending_deployments": runs.pendingDeployments,
  "pipelines.review_deployment": runs.reviewDeployment,
};

export function call({ action, args, creds, idempotencyKey }: { action: string; args: Record<string, any>; creds: Creds; idempotencyKey: string }, ctx) {
  const run = Object.prototype.hasOwnProperty.call(ACTIONS, action) ? ACTIONS[action] : undefined;
  if (run === undefined) return unknownAction(action);
  return run(api(ctx, creds), args ?? {}, idempotencyKey ?? "");
}

export function accounts({ creds }: { creds: Creds }, ctx) {
  const login = textOf(get(api(ctx, creds), "/user"), ["login"]);
  return [{ id: login, label: login, settings: { login } }];
}
