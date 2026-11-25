import {
  type Creds,
  type Language,
  bearer,
  checked,
  connect,
  fail,
  json,
  listIn,
  optionalText,
  pick,
  query,
  send,
  setting,
  settingField,
  text,
  textOf,
  tokenField,
  unknownAction,
} from "sdk/connectors/index.ts";
import { binding, options, quality } from "sdk/connectors/contracts.ts";

export const URL_SETTING = "url";
export const ORGANIZATION = "organization";
const PAGE = "100";
const MOST_PAGES = 10;
const ISSUES = "50";
const GRADES = ["A", "B", "C", "D", "E"];
const MEASURES = [
  "new_reliability_rating",
  "new_security_rating",
  "new_maintainability_rating",
  "new_bugs",
  "new_vulnerabilities",
  "new_code_smells",
  "new_security_hotspots",
  "new_security_hotspots_reviewed",
  "new_coverage",
  "new_duplicated_lines_density",
];
const TYPES: Record<string, string> = { BUG: "bug", VULNERABILITY: "vulnerability", CODE_SMELL: "code_smell", SECURITY_HOTSPOT: "hotspot" };
const SEVERITIES = ["blocker", "critical", "major", "minor", "info"];

type Ctx = { config: { url: string } } & Record<string, any>;

type Api = { ctx: Ctx; token: string; base: string; organization?: string };

type Target = { project: string; pullRequest?: string; branch?: string; commit?: string };

export function baseOf(entered: string | undefined, fallback: string): string {
  const bare = (entered ?? "").trim().replace(/\/+$/, "");
  if (bare === "") return fallback.replace(/\/+$/, "");
  return /^https?:\/\//.test(bare) ? bare : `https://${bare}`;
}

function fields(ctx: Ctx, language: Language) {
  const url = settingField(URL_SETTING, pick(language, "SonarQube address", "Dirección de SonarQube", "Endereço do SonarQube"), "", ctx.config.url);
  const organization = settingField(
    ORGANIZATION,
    pick(language, "Organization (SonarQube Cloud)", "Organización (SonarQube Cloud)", "Organização (SonarQube Cloud)"),
    "",
  );
  const help = pick(
    language,
    "In SonarQube, open My Account › Security and generate a user token.",
    "En SonarQube, abre Mi cuenta › Seguridad y genera un token de usuario.",
    "No SonarQube, abra Minha conta › Segurança e gere um token de usuário.",
  );
  return [url, organization, tokenField("token", "Token", help, true)];
}

export function provider({ language }: { language?: Language }, ctx: Ctx) {
  return {
    name: "SonarQube",
    logo: "logo.svg",
    capabilities: ["quality"],
    auth: { kind: "token", fields: fields(ctx, language ?? "pt-BR"), form: "bearer" },
    actions: [...quality(), options("quality")],
  };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(language, "Your SonarQube account", "Tu cuenta de SonarQube", "Sua conta SonarQube"),
    group: "quality",
    button: connect(language),
    site: pick(language, "SonarQube", "de SonarQube", "do SonarQube"),
    choose_title: pick(language, "SonarQube account", "Cuenta de SonarQube", "Conta do SonarQube"),
    hidden: false,
    bindings: [binding("quality", "repository", pick(language, "SonarQube project", "Proyecto de SonarQube", "Projeto do SonarQube"))],
  };
}

function apiOf(ctx: Ctx, creds: Creds): Api {
  return { ctx, token: creds.token, base: baseOf(setting(creds, URL_SETTING), ctx.config.url), organization: setting(creds, ORGANIZATION) };
}

function answer(api: Api, path: string, pairs: [string, string][]) {
  const url = `${api.base}${path}${pairs.length === 0 ? "" : `?${query(pairs)}`}`;
  return send(api.ctx, { method: "GET", url, headers: bearer(api.token) });
}

function get(api: Api, path: string, pairs: [string, string][]) {
  return json(checked(answer(api, path, pairs)));
}

function scoped(target: Target): [string, string][] {
  if (target.pullRequest) return [["pullRequest", target.pullRequest]];
  return target.branch ? [["branch", target.branch]] : [];
}

function projects(api: Api, wanted: string) {
  const found: { value: string; label: string }[] = [];
  for (let page = 1; page <= MOST_PAGES; page++) {
    const pairs: [string, string][] = [["qualifiers", "TRK"], ["ps", PAGE], ["p", String(page)]];
    if (wanted !== "") pairs.push(["q", wanted]);
    if (api.organization) pairs.push(["organization", api.organization]);
    const listed = listIn(get(api, "/api/components/search", pairs)?.components);
    for (const component of listed) {
      const key = textOf(component, ["key"]);
      const name = textOf(component, ["name"]);
      if (key !== "") found.push({ value: key, label: name !== "" && name !== key ? `${name} · ${key}` : key });
    }
    if (listed.length < Number(PAGE)) break;
  }
  return found;
}

export function gradeOf(rating: string | undefined): string {
  const at = Math.round(Number(rating ?? "1")) - 1;
  return GRADES[Math.max(0, Math.min(GRADES.length - 1, Number.isFinite(at) ? at : 0))];
}

function measures(api: Api, target: Target): Record<string, string> {
  const found = get(api, "/api/measures/component", [["component", target.project], ["metricKeys", MEASURES.join(",")], ...scoped(target)]);
  const values: Record<string, string> = {};
  for (const measure of listIn(found?.component?.measures)) {
    const value = measure?.value ?? measure?.period?.value ?? measure?.periods?.[0]?.value;
    if (typeof measure?.metric === "string" && value !== undefined) values[measure.metric] = String(value);
  }
  return values;
}

function numberOf(values: Record<string, string>, name: string): number | null {
  const found = Number(values[name]);
  return values[name] === undefined || !Number.isFinite(found) ? null : found;
}

function fileOf(component: string): string | null {
  const at = component.indexOf(":");
  return at < 0 ? null : component.slice(at + 1) || null;
}

function issues(api: Api, target: Target, base: string) {
  const found = get(api, "/api/issues/search", [["componentKeys", target.project], ["resolved", "false"], ["ps", ISSUES], ...scoped(target)]);
  const listed = listIn(found?.issues).map((issue) => {
    const severity = String(issue?.severity ?? "").toLowerCase();
    const key = textOf(issue, ["key"]);
    return {
      key,
      type: TYPES[String(issue?.type ?? "")] ?? "code_smell",
      severity: SEVERITIES.includes(severity) ? severity : "info",
      message: textOf(issue, ["message"]),
      file: fileOf(textOf(issue, ["component"])),
      line: Number.isInteger(issue?.line) ? issue.line : null,
      url: `${base}/project/issues?id=${encodeURIComponent(target.project)}&open=${encodeURIComponent(key)}`,
    };
  });
  return { listed, total: Number.isInteger(found?.total) ? found.total : listed.length };
}

function metricNames(api: Api): Record<string, string> {
  const names: Record<string, string> = {};
  for (const metric of listIn(get(api, "/api/metrics/search", [["ps", "500"]])?.metrics)) {
    const key = textOf(metric, ["key"]);
    const name = textOf(metric, ["name"]);
    if (key !== "" && name !== "") names[key] = name;
  }
  return names;
}

function conditionsOf(api: Api, status: any) {
  const listed = listIn(status?.conditions);
  const names = listed.length === 0 ? {} : metricNames(api);
  return listed.map((condition) => ({
    metric: textOf(condition, ["metricKey"]),
    label: names[textOf(condition, ["metricKey"])] ?? textOf(condition, ["metricKey"]),
    value: textOf(condition, ["actualValue"]),
    threshold: textOf(condition, ["errorThreshold"]),
    passed: condition?.status !== "ERROR",
  }));
}

function dashboard(api: Api, target: Target): string {
  const scope = scoped(target).map(([name, value]) => `&${name}=${encodeURIComponent(value)}`).join("");
  return `${api.base}/dashboard?id=${encodeURIComponent(target.project)}${scope}`;
}

function analyzedCommit(api: Api, target: Target): string | undefined {
  if (!target.pullRequest) return undefined;
  const listed = listIn(get(api, "/api/project_pull_requests/list", [["project", target.project]])?.pullRequests);
  const found = listed.find((pull) => String(pull?.key ?? "") === target.pullRequest);
  const sha = textOf(found, ["commit", "sha"]);
  return sha === "" ? undefined : sha;
}

function stale(api: Api, target: Target): boolean {
  if (!target.commit) return false;
  const sha = analyzedCommit(api, target);
  return sha !== undefined && !sha.startsWith(target.commit) && !target.commit.startsWith(sha);
}

export function report(api: Api, target: Target) {
  const url = dashboard(api, target);
  if (stale(api, target)) return { status: "pending", url };
  const raw = answer(api, "/api/qualitygates/project_status", [["projectKey", target.project], ...scoped(target)]);
  if (raw.status === 404) return { status: "pending", url };
  const status = json(checked(raw))?.projectStatus;
  const gate = String(status?.status ?? "NONE");
  if (gate === "NONE") return { status: "pending", url };
  const values = measures(api, target);
  const found = issues(api, target, api.base);
  return {
    status: "done",
    gate: gate === "ERROR" ? "failed" : "passed",
    ratings: {
      reliability: gradeOf(values.new_reliability_rating),
      security: gradeOf(values.new_security_rating),
      maintainability: gradeOf(values.new_maintainability_rating),
    },
    counts: {
      bugs: numberOf(values, "new_bugs") ?? 0,
      vulnerabilities: numberOf(values, "new_vulnerabilities") ?? 0,
      smells: numberOf(values, "new_code_smells") ?? 0,
      hotspots: numberOf(values, "new_security_hotspots") ?? 0,
      hotspotsReviewed: numberOf(values, "new_security_hotspots_reviewed") ?? 0,
    },
    coverage: numberOf(values, "new_coverage"),
    duplication: numberOf(values, "new_duplicated_lines_density"),
    conditions: conditionsOf(api, status),
    issues: found.listed,
    total: found.total,
    url,
    analyzedAt: null,
  };
}

function targetOf(args: Record<string, unknown>): Target {
  return {
    project: text(args, "project"),
    pullRequest: optionalText(args, "pullRequest"),
    branch: optionalText(args, "branch"),
    commit: optionalText(args, "commit"),
  };
}

export function call({ action, args, creds }: { action: string; args: Record<string, unknown>; creds: Creds }, ctx: Ctx) {
  const api = apiOf(ctx, creds);
  if (action === "quality.report") return report(api, targetOf(args ?? {}));
  if (action === "quality.options") return projects(api, (optionalText(args ?? {}, "query") ?? "").trim());
  return unknownAction(action);
}

export function accounts({ creds }: { creds: Creds }, ctx: Ctx) {
  const api = apiOf(ctx, creds);
  if (get(api, "/api/authentication/validate", [])?.valid !== true) fail("auth", "the SonarQube token is not valid");
  const login = textOf(get(api, "/api/users/current", []), ["login"]);
  const host = api.base.replace(/^https?:\/\//, "");
  const label = [login, api.organization ?? host].filter(Boolean).join(" · ");
  const settings: Record<string, unknown> = { [URL_SETTING]: api.base };
  if (api.organization) settings[ORGANIZATION] = api.organization;
  return [{ id: `sonarqube-${host}-${api.organization ?? ""}`, label: label || "SonarQube", settings }];
}
