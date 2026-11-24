import { step, issue, markdown, form, actions } from "sdk";
import { attempt, defined, effects, object, planOf, sourcesOf, waitOptions } from "./context.js";

const UNTRACKED = "Nenhum rastreador de issues está conectado, então o item segue sem issue.";

const CHOOSE = "O rastreador tem mais de um projeto. Escolha onde criar a issue deste item.";

const RECHECK = "recheck_statuses";

const KEEP = "keep_statuses";

function shown(s) {
  if (s.issue) return [issue(s.issue), ...(s.unknownStatuses ? [markdown(missingText(s.unknownStatuses, s.trackerStatuses ?? []))] : [])];
  return s.reason ? [markdown(s.reason)] : [];
}

function projectKey(found) {
  const entry = object(found);
  return entry.key ?? entry.id ?? null;
}

function projectFields(projects, opts) {
  const options = projects.map((entry) => {
    const key = projectKey(entry);
    return { value: key, label: entry.name && entry.name !== key ? `${key} · ${entry.name}` : key };
  });
  const fields = [{ name: "project", label: "Projeto", kind: "select", options }];
  if (!opts.type) fields.push({ name: "type", label: "Tipo de issue", placeholder: "Deixe vazio para usar o padrão do rastreador" });
  return fields;
}

async function chooseProject(ctx, opts, projects) {
  const keys = projects.map(projectKey);
  const view = [markdown(CHOOSE), form(projectFields(projects, opts), { title: "Onde criar a issue", submit: "Criar issue" })];
  for (;;) {
    const answer = await ctx.waitHuman(view, waitOptions(opts));
    const values = object(answer.values);
    if (keys.includes(values.project)) {
      const type = String(values.type ?? "").trim();
      return defined({ project: values.project, type: type || undefined, by: answer.by });
    }
  }
}

async function destination(ctx, opts, options) {
  const given = opts.project ?? ctx.item.project;
  if (given) return { project: given };
  const listed = await attempt(() => ctx.call("tracker", "projects", {}, options));
  if (listed === null) return null;
  const projects = (Array.isArray(listed) ? listed : []).filter((entry) => projectKey(entry));
  if (projects.length === 0) return {};
  if (projects.length === 1) return { project: projectKey(projects[0]) };
  return chooseProject(ctx, opts, projects);
}

function nameOf(found) {
  return typeof found === "string" ? found : object(found).name;
}

function missingText(missing, known) {
  const listed = known.length > 0 ? known.join(", ") : "nenhum";
  return `Os status ${missing.join(", ")} do workflow não existem no rastreador deste projeto. Status que existem: ${listed}. Ajuste o workflow ou o rastreador; até lá o item não muda de status lá.`;
}

async function trackerStatuses(ctx, project, options) {
  const listed = await attempt(() => ctx.call("tracker", "statuses", { project }, options));
  return (Array.isArray(listed) ? listed : []).map(nameOf).filter((name) => typeof name === "string" && name);
}

export async function checkStatuses(ctx, opts, project, options) {
  const wanted = ctx.workflow ? ctx.workflow.statuses : [];
  if (!project || wanted.length === 0 || opts.checkStatuses === false) return {};
  for (;;) {
    const known = await trackerStatuses(ctx, project, options);
    if (known.length === 0) return {};
    const missing = wanted.filter((name) => !known.some((found) => found.toLowerCase() === name.toLowerCase()));
    if (missing.length === 0) return {};
    const text = missingText(missing, known);
    await ctx.impediment(true, text);
    const buttons = [
      { id: RECHECK, label: "Verificar de novo", icon: "refresh", variant: "primary" },
      { id: KEEP, label: "Seguir assim", icon: "right" },
    ];
    const answer = await ctx.waitHuman([markdown(text), actions(buttons)], waitOptions(opts));
    await ctx.impediment(false);
    if (answer.action !== RECHECK) return { unknownStatuses: missing, trackerStatuses: known };
  }
}

async function subtasksOf(ctx, opts, plan, project, parent, options) {
  const made = [];
  for (const task of opts.subtasks === false ? [] : plan.subtasks) {
    const child = defined({ project, title: task.title, description: task.details, parent });
    const found = await ctx.call("tracker", "create_issue", child, options);
    made.push({ issue: found.ref, title: task.title });
  }
  return made;
}

export default step({
  name: "ticket",
  label: "Issue",
  icon: "sTicket",
  needs: "tracker",
  doing: "Criando issue",
  view: shown,
  async run(ctx, opts) {
    const tracked = sourcesOf(ctx, "issue")[0];
    const connection = opts.connection ?? ctx.item.connection ?? undefined;
    const options = { ...effects({ ...opts, connection }), ...(connection ? {} : { optional: true }) };
    const existing = opts.issue ?? ctx.item.issue ?? tracked?.subject ?? object(ctx.output(ctx.step)).issue;
    if (existing) {
      const checked = await checkStatuses(ctx, opts, opts.project ?? ctx.item.project, options);
      return ctx.pass({ issue: existing, created: false, ...checked });
    }
    const chosen = await destination(ctx, opts, options);
    if (chosen === null) return ctx.skip({ created: false, reason: UNTRACKED });
    const plan = planOf(ctx, opts);
    const args = defined({
      project: chosen.project,
      title: plan.title,
      description: opts.description ?? (plan.story || plan.text),
      type: opts.type ?? chosen.type,
      labels: opts.labels,
      parent: opts.parent,
    });
    const created = await ctx.call("tracker", "create_issue", args, options);
    if (!created) return ctx.skip({ created: false, reason: UNTRACKED });
    const subtasks = await subtasksOf(ctx, opts, plan, args.project, created.ref, options);
    const checked = await checkStatuses(ctx, opts, args.project, options);
    return ctx.pass(
      defined({
        issue: created.ref,
        url: created.url,
        title: created.title ?? plan.title,
        created: true,
        project: args.project,
        subtasks: subtasks.length > 0 ? subtasks : undefined,
        ...checked,
      }),
    );
  },
});
