import { bindingOf, codeOf } from "sdk/bindings.js";
import { markdown, actions } from "sdk";

export const FINISHED = ["passed", "failed", "cancelled", "skipped"];
export const SUCCEEDED = ["passed", "skipped"];

const LIMIT = 8000;

export const LANGUAGES = { en: "English", es: "Spanish", "pt-BR": "Brazilian Portuguese" };

export function languageOf(ctx) {
  const chosen = ctx?.item?.language;
  return Object.hasOwn(LANGUAGES, chosen ?? "") ? chosen : "pt-BR";
}

export function inLanguage(ctx, text) {
  return `${text}\n\nReply and write everything in ${LANGUAGES[languageOf(ctx)]}.`;
}

const HEADINGS = {
  description: ["description", "descrição", "descricao", "descripción", "descripcion"],
  criteria: ["acceptance criteria", "critérios de aceite", "criterios de aceite", "criterios de aceptación", "criterios de aceptacion"],
  subtasks: ["subtasks", "subtarefas", "subtareas"],
  infra: ["infrastructure", "infraestrutura", "infraestructura", "manual tasks", "tarefas manuais", "tareas manuales"],
};

const CRITERIA = { en: "Acceptance criteria", es: "Criterios de aceptación", "pt-BR": "Critérios de aceite" };

function heading(found, kind) {
  return HEADINGS[kind].map((name) => found[name]).find((text) => text !== undefined) ?? "";
}

export function defined(fields) {
  const chosen = {};
  for (const [key, value] of Object.entries(fields)) {
    if (value !== undefined && value !== null) chosen[key] = value;
  }
  return chosen;
}

export function effects(opts) {
  return defined({ connection: opts.connection, timeout: opts.timeout, retry: opts.retry });
}

export function agentOptions(opts, extra) {
  return defined({
    model: opts.model,
    mode: opts.mode,
    askTimeout: opts.askTimeout,
    timeout: opts.timeout,
    retry: opts.retry,
    cloud: opts.cloud,
    ...extra,
  });
}

export function environmentOf(environments, base) {
  const found = (Array.isArray(environments) ? environments : []).find((env) => env && env.branch && env.branch === base);
  return found ? found.id : undefined;
}

export function cloudFor(opts, base) {
  if (!opts.cloud) return undefined;
  const asked = opts.cloud && typeof opts.cloud === "object" ? opts.cloud : typeof opts.cloud === "string" ? { connection: opts.cloud } : {};
  const environment = asked.environment ?? opts.environment ?? environmentOf(opts.environments, base);
  return environment ? { ...asked, environment } : opts.cloud;
}

export function tail(text, limit) {
  const value = String(text ?? "");
  const most = limit ?? LIMIT;
  return value.length > most ? value.slice(value.length - most) : value;
}

export function object(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value : {};
}

export function source(ctx, opts, kind) {
  const ids = object(opts.sources);
  return object(ctx.output(ids[kind] ?? kind));
}

function sections(body) {
  const found = {};
  let name = "";
  for (const line of body.split("\n")) {
    const heading = /^##\s+(.+?)\s*$/.exec(line);
    if (heading) {
      name = heading[1].toLowerCase();
      found[name] = [];
    } else if (name) {
      found[name].push(line);
    }
  }
  return Object.fromEntries(Object.entries(found).map(([key, lines]) => [key, lines.join("\n").trim()]));
}

function subtasksOf(text) {
  return String(text ?? "")
    .split(/^###\s+/m)
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      const [first, ...rest] = part.split("\n");
      return { title: first.trim(), details: rest.join("\n").trim() };
    });
}

const KEYWORDS = {
  en: { given: "Given", when: "when", then: "then", as: "As", want: "I want", so: "so that" },
  "pt-BR": { given: "Dado", when: "quando", then: "então", as: "Como", want: "eu quero", so: "para que" },
  es: { given: "Dado", when: "cuando", then: "entonces", as: "Como", want: "quiero", so: "para que" },
};

const SPOKEN = {
  given: "given|dado(?: que)?",
  when: "when|quando|cuando",
  then: "then|ent[aã]o|entonces",
  as: "as(?: an?)?|como(?: um| uma| un| una)?",
  want: "i want|eu quero|quero|quiero|yo quiero",
  so: "so that|para que",
};

function spoken(kind, at) {
  const words = SPOKEN[kind];
  return new RegExp(`${at}\\**(${words})\\**(?=[\\s,:]|$)`, "iu");
}

function keyword(line, kind, at, named) {
  return line.replace(spoken(kind, at), (_, lead, word) => {
    const english = /^(as|given)\b/i.test(word);
    const kept = !english || named === KEYWORDS.en[kind];
    const article = (kind === "as" || kind === "given") && kept ? word.replace(/^\S+/, "") : "";
    return `${lead}**${named}**${article}`;
  });
}

function scenario(line, named) {
  let shaped = keyword(line, "given", "^(\\s*[-*]\\s+)", named.given);
  if (shaped === line) return line;
  shaped = keyword(shaped, "when", "(,\\s*)", named.when);
  return keyword(shaped, "then", "(,\\s*)", named.then);
}

function storyLine(line, named) {
  let shaped = keyword(line, "as", "^(\\s*)", named.as);
  if (shaped === line) return line;
  shaped = keyword(shaped, "want", "(,\\s*)", named.want);
  return keyword(shaped, "so", "(\\s+|,\\s*)", named.so);
}

export function localizeStory(text, language) {
  const named = KEYWORDS[language] ?? KEYWORDS["pt-BR"];
  return String(text ?? "")
    .split("\n")
    .map((line) => storyLine(scenario(line, named), named))
    .join("\n");
}

export function parsePlan(text, language) {
  const lines = localizeStory(text, language).split("\n");
  const at = lines.findIndex((line) => /^# \S/.test(line));
  if (at < 0) return { title: "", story: "", subtasks: [], infra: [], text: String(text ?? "").trim() };
  const title = lines[at].slice(2).trim();
  const found = sections(lines.slice(at + 1).join("\n"));
  const description = heading(found, "description");
  const criteria = heading(found, "criteria");
  const named = CRITERIA[language] ?? CRITERIA["pt-BR"];
  const story = [description, criteria ? `## ${named}\n${criteria}` : ""].filter(Boolean).join("\n\n");
  const subtasks = subtasksOf(heading(found, "subtasks"));
  return { title, story, subtasks, infra: subtasksOf(heading(found, "infra")), text: lines.slice(at).join("\n").trim() };
}

export function planOf(ctx, opts) {
  const found = source(ctx, opts, "plan");
  return {
    title: opts.title ?? found.title ?? ctx.item.title ?? "",
    text: opts.plan ?? found.plan ?? "",
    story: found.story ?? "",
    subtasks: Array.isArray(found.subtasks) ? found.subtasks : [],
    infra: Array.isArray(found.infra) ? found.infra : [],
    designs: Array.isArray(found.designs) ? found.designs.filter((link) => typeof link === "string") : [],
  };
}

export function summaryLine(text) {
  const found = String(text ?? "").match(/^\**(?:Summary|Resumo|Resumen):?\**:?\s*(.+)$/im);
  if (found) return found[1].trim();
  const first = String(text ?? "").split(/\n\s*\n/).map((part) => part.trim()).find((part) => part && !part.startsWith("#"));
  return first ? first.split("\n")[0].slice(0, 280) : "";
}

export function sourcesOf(ctx, kind) {
  const input = object(ctx.item.input);
  const listed = Array.isArray(input.sources) ? input.sources : Array.isArray(ctx.item.sources) ? ctx.item.sources : [];
  return listed.filter((entry) => entry && entry.kind === kind && typeof entry.subject === "string" && entry.subject);
}

export function gitOf(ctx, repository) {
  return object(object(ctx.item.git)[repository ?? ""]);
}

export function issueOf(ctx, opts) {
  return opts.issue ?? source(ctx, opts, "ticket").issue ?? ctx.item.issue ?? sourcesOf(ctx, "issue")[0]?.subject ?? null;
}

export function branchOf(ctx, opts) {
  return opts.branch ?? source(ctx, opts, "develop").branch ?? ctx.item.branch ?? null;
}

export function pullRequestOf(ctx, opts) {
  const found = source(ctx, opts, "pr");
  if (typeof opts.pr === "string") return { ...found, pr: opts.pr };
  return found.pr ? found : (sourcePullRequests(ctx)[0] ?? null);
}

export function pullRequestsOf(ctx, opts) {
  if (typeof opts.pr === "string") return [{ ...source(ctx, opts, "pr"), prs: undefined, pr: opts.pr }];
  const found = source(ctx, opts, "pr");
  if (Array.isArray(found.prs) && found.prs.length > 0) return found.prs.filter((entry) => entry && entry.pr);
  return found.pr ? [found] : sourcePullRequests(ctx);
}

function belongs(entry, repository) {
  return entry.repository === repository.id || (Boolean(repository.code) && entry.repository === repository.code.repo);
}

export function sourcePullRequestOf(ctx, target) {
  const found = sourcesOf(ctx, "pull_request");
  const known = object(target);
  const sole = found.length === 1 && !found[0].repository && repositoriesOf(ctx).length <= 1 ? found[0] : null;
  if (!known.id) return found.length === 1 ? found[0] : null;
  return found.find((entry) => belongs(entry, known)) ?? sole;
}

function fromSource(ctx, entry) {
  const repositories = repositoriesOf(ctx);
  const sole = !entry.repository && repositories.length === 1 ? repositories[0] : null;
  const repository = repositories.find((known) => belongs(entry, known)) ?? sole;
  const coded = repository && repository.code ? repository.code : null;
  const code = entry.connection ? { ...(coded ?? {}), connection: entry.connection } : coded;
  const git = gitOf(ctx, repository ? repository.id : entry.repository);
  return defined({ repository: repository ? repository.id : entry.repository, name: repository ? repository.name : undefined, code, pr: entry.subject, head: git.head, base: git.base });
}

export function sourcePullRequests(ctx) {
  return sourcesOf(ctx, "pull_request").map((entry) => fromSource(ctx, entry));
}

function folderName(directory) {
  const parts = String(directory ?? "").split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? "";
}

export function nameOf(repository) {
  const code = object(codeOf(repository));
  return code.repo || folderName(object(repository).directory);
}

export function repositoriesOf(ctx) {
  const application = object(ctx.item.application);
  const listed = Array.isArray(application.repositories) ? application.repositories : [];
  return listed
    .filter((repository) => repository && repository.id)
    .map((repository) => ({ ...repository, name: nameOf(repository), code: codeOf(repository) }));
}

function touched(changes, target) {
  const found = changes.find((entry) => entry.id === target.id);
  return !found || found.changed !== false;
}

export function codeTargets(ctx, opts) {
  const repositories = repositoriesOf(ctx);
  if (repositories.length === 0) return [{ id: "", name: "", code: null }];
  const coded = repositories.filter((repository) => repository.code && repository.code.repo);
  if (coded.length === 0) return repositories.length === 1 ? [{ ...repositories[0], code: null }] : [];
  const changes = source(ctx, opts, "develop").repositories;
  if (!Array.isArray(changes)) return coded;
  const changed = coded.filter((target) => touched(changes, target));
  return changed.length > 0 ? changed : coded;
}

export function codeOptions(opts, target) {
  const code = object(object(target).code);
  return { ...opts, connection: opts.connection ?? code.connection };
}

export function pipelineOptions(opts, target) {
  const code = object(object(target).code);
  return { ...opts, connection: opts.connection ?? (code.pipelines ? code.connection : undefined) };
}

export function repoOf(target) {
  return object(object(target).code).repo ?? undefined;
}

export function targetFields(target) {
  return defined({ repository: target.id || target.repository, name: target.name, code: target.code });
}

export async function eachOf(ctx, targets, work) {
  if (targets.length === 1) return [await work(ctx, targets[0], 0)];
  return ctx.all(targets.map((target, index) => (branch) => work(branch, target, index)));
}

export function waitOptions(opts) {
  return defined({ timeout: opts.waitTimeout });
}

function comment(entry) {
  const where = entry.path ? `${entry.path}${entry.line ? `:${entry.line}` : ""}: ` : "";
  return `- ${where}${entry.body ?? ""}`;
}

export function feedbackText(from) {
  if (!from) return "";
  const data = object(from.data);
  const parts = [`Step ${from.step} sent the work back.`];
  if (data.summary) parts.push(String(data.summary));
  if (data.error) parts.push(String(data.error));
  if (data.body) parts.push(String(data.body));
  if (Array.isArray(data.comments) && data.comments.length > 0) parts.push(data.comments.map(comment).join("\n"));
  if (data.output) parts.push(`\`\`\`\n${tail(data.output)}\n\`\`\``);
  return parts.join("\n\n");
}

export const DEPENDABOT = "dependabot[bot]";

function ours(ctx, run) {
  if (!run || run.event === "dynamic" || String(run.file ?? "").startsWith("dynamic/")) return false;
  return run.actor !== DEPENDABOT;
}

async function neverRan(ctx, opts, wanted) {
  try {
    const ever = await ctx.call("pipelines", "runs", defined({ repo: wanted.repo, pipeline: opts.pipeline, limit: 20 }), effects(opts));
    return Array.isArray(ever) && !ever.some((run) => ours(ctx, run));
  } catch {
    return false;
  }
}

export async function findRun(ctx, opts, wanted) {
  const args = defined({ ref: wanted.ref, repo: wanted.repo, pipeline: opts.pipeline, limit: 10 });
  const patience = opts.patience ?? 10;
  for (let tries = 1; ; tries += 1) {
    const runs = await ctx.call("pipelines", "runs", args, effects(opts));
    const fresh = (Array.isArray(runs) ? runs : []).find((run) => ours(ctx, run) && !wanted.seen.includes(run.ref));
    if (fresh) return fresh;
    if (wanted.optional && tries >= patience) return null;
    if (wanted.optional && tries === 2 && (await neverRan(ctx, opts, wanted))) return null;
    await ctx.sleep(opts.every ?? 30000);
  }
}

export async function settle(ctx, opts, run, until, events) {
  const target = defined({ capability: "pipelines", action: "run", args: { run }, connection: opts.connection });
  for (;;) {
    const waits = events.map((kind) => ctx.event(kind, { run }));
    const polled = ctx.poll(target, opts.every ?? 30000, defined({ until: { state: until }, timeout: opts.timeout }));
    const raced = await ctx.race([...waits, polled]);
    const current = raced.kind === "event" ? await ctx.call("pipelines", "run", { run }, effects(opts)) : raced.value;
    if (until.includes(current.state)) return current;
  }
}

export async function failedLogs(ctx, opts, run) {
  const failed = (Array.isArray(run.jobs) ? run.jobs : []).filter((job) => job.state === "failed");
  const texts = [];
  for (const job of failed) {
    const log = await ctx.call("pipelines", "job_log", { run: run.ref, job: job.id }, effects(opts));
    texts.push(`## ${job.name}\n${tail(object(log).text, 4000)}`);
  }
  return texts.join("\n\n");
}

const CHOICES = [
  { id: "retry", label: "I connected it, try again", icon: "refresh", variant: "primary" },
  { id: "skip", label: "Skip step", icon: "right" },
  { id: "cancel", label: "Cancel item", icon: "stop", variant: "danger" },
];

export async function missingConnection(ctx, opts, capability, what) {
  const reason = `connect an account for ${capability}`;
  const text = `**A ${what} connection is missing.** Connect an account in Settings › Connections and try again, or continue without this step.`;
  await ctx.impediment(true, reason);
  const answer = await ctx.waitHuman([markdown(text), actions(CHOICES)], waitOptions(opts));
  await ctx.impediment(false);
  const chosen = object(answer).action;
  return chosen === "skip" || chosen === "cancel" ? chosen : "retry";
}

export async function attempt(work) {
  try {
    return await work();
  } catch {
    return undefined;
  }
}

export function boundPullRequests(ctx, opts, capability) {
  const repositories = repositoriesOf(ctx);
  const commits = source(ctx, opts, "develop");
  return pullRequestsOf(ctx, opts).flatMap((entry) => {
    const sole = repositories.length === 1 ? repositories[0] : null;
    const repository = repositories.find((known) => known.id === entry.repository) ?? sole;
    const bound = repository ? bindingOf(repository, capability) : null;
    if (!bound) return [];
    const changed = (Array.isArray(commits.repositories) ? commits.repositories : []).find((tree) => object(tree).id === repository.id);
    const commit = object(changed).commit ?? (repositories.length === 1 ? commits.commit : undefined);
    const number = entry.number ?? String(entry.pr ?? "").split("#").pop();
    return [defined({ ...entry, repository: repository.id, name: entry.name ?? repository.name, bound, number: number ? String(number) : undefined, commit })];
  });
}

export function trackerOptions(ctx, opts) {
  const connection = opts.trackerConnection ?? ctx.item.connection ?? sourcesOf(ctx, "issue")[0]?.connection ?? undefined;
  return { ...effects({ ...opts, connection }), ...(connection ? {} : { optional: true }) };
}

function listed(value) {
  return Array.isArray(value) ? value.map(object) : [];
}

function subtaskLines(found) {
  return listed(found.subtasks).map((task) => `- ${task.ref ?? ""} ${task.title ?? ""}${task.status ? ` (${task.status})` : ""}`.trim());
}

function commentLines(found) {
  return listed(found.comments)
    .filter((comment) => String(comment.body ?? "").trim())
    .map((comment) => `**${object(comment.author).name ?? ""}**${comment.at ? ` · ${comment.at}` : ""}\n${String(comment.body).trim()}`);
}

export function issueBrief(found) {
  const issue = object(found);
  if (!issue.ref && !issue.title) return "";
  const facts = [
    issue.type ? `Type: ${issue.type}` : "",
    issue.status ? `Status: ${issue.status}` : "",
    issue.parent ? `Parent: ${issue.parent}` : "",
    Array.isArray(issue.labels) && issue.labels.length > 0 ? `Labels: ${issue.labels.join(", ")}` : "",
    issue.url ? `Link: ${issue.url}` : "",
  ].filter(Boolean);
  const parts = [`## Issue ${issue.ref ?? ""}: ${issue.title ?? ""}`.trim(), facts.join("\n")];
  if (String(issue.description ?? "").trim()) parts.push(`### Description\n\n${String(issue.description).trim()}`);
  const subtasks = subtaskLines(issue);
  if (subtasks.length > 0) parts.push(`### Subtasks\n\n${subtasks.join("\n")}`);
  const comments = commentLines(issue);
  if (comments.length > 0) parts.push(`### Comments\n\n${comments.join("\n\n")}`);
  return parts.filter(Boolean).join("\n\n");
}

export async function readIssue(ctx, opts, issue) {
  if (!issue) return null;
  const found = await attempt(() => ctx.call("tracker", "issue", { issue: String(issue) }, trackerOptions(ctx, opts)));
  return found && typeof found === "object" ? found : null;
}

const MOST_LINKS = 5;

export function linksIn(...texts) {
  const found = texts.flatMap((text) => String(text ?? "").match(/https?:\/\/[^\s<>()\]"'`]+/g) ?? []);
  return [...new Set(found.map((link) => link.replace(/[.,;:!?*_]+$/, "")))].slice(0, MOST_LINKS);
}

export async function readDesigns(ctx, opts, links) {
  const read = [];
  for (const url of links) {
    const found = await attempt(() => ctx.call("design", "frame", { url }, { ...effects(opts), optional: true }));
    if (found && typeof found === "object" && found.ref) read.push(found);
  }
  return read;
}

const MOST_PICTURES = 3;

export const DESIGNED = "These designs are linked from the item and were read for you from the design tool. Use their texts and structure as the source of truth for the screens.";

export const PICTURED = "The rendered screens are attached as images, in the same order.";

export function picturesOf(frames) {
  return frames
    .map((frame) => object(frame.picture))
    .filter((picture) => typeof picture.media_type === "string" && typeof picture.data === "string" && picture.data !== "")
    .slice(0, MOST_PICTURES)
    .map((picture) => ({ media_type: picture.media_type, data: picture.data }));
}

export function designNote(frames) {
  const brief = designBrief(frames);
  if (!brief) return "";
  return [picturesOf(frames).length > 0 ? `${DESIGNED} ${PICTURED}` : DESIGNED, brief].join("\n\n");
}

export function designBrief(frames) {
  return frames
    .map((frame) => {
      const texts = Array.isArray(frame.texts) ? frame.texts.filter(Boolean) : [];
      const parts = [`## Design: ${frame.name || frame.ref}${frame.file ? ` (${frame.file})` : ""}`, `Link: ${frame.url}`];
      if (frame.image) parts.push(`Rendered image: ${frame.image}`);
      if (texts.length > 0) parts.push(`### Texts on the screen\n\n${texts.map((line) => `- ${line.replace(/\n+/g, " ")}`).join("\n")}`);
      if (frame.outline) parts.push(`### Structure\n\n${frame.outline}`);
      return parts.join("\n\n");
    })
    .join("\n\n");
}
