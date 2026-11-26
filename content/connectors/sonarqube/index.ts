import {
  type Creds,
  type Language,
  type Response,
  ActionError,
  bearer,
  checked,
  connect,
  fail,
  listIn,
  optionalText,
  pick,
  query,
  send,
  setting,
  settingField,
  siteOf,
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
const IMPACT_TYPES: Record<string, string> = { RELIABILITY: "bug", SECURITY: "vulnerability", MAINTAINABILITY: "code_smell" };
const IMPACT_SEVERITIES: Record<string, string> = { BLOCKER: "blocker", HIGH: "critical", MEDIUM: "major", LOW: "minor", INFO: "info" };

type Ctx = { config: { url: string; cloud?: string } } & Record<string, any>;

type Scheme = "bearer" | "basic";

type Api = { ctx: Ctx; token: string; base: string; organization?: string; scheme: Scheme };

type Target = { project: string; pullRequest?: string; branch?: string; commit?: string };

export function baseOf(entered: string | undefined, fallback: string): string {
  const bare = (entered ?? "").trim().replace(/[?#].*$/, "").replace(/\/+$/, "").replace(/\/api$/, "");
  if (bare === "") return fallback.replace(/\/+$/, "");
  return /^https?:\/\//i.test(bare) ? bare : `https://${bare}`;
}

export function isCloud(base: string, hosts: string | undefined): boolean {
  const host = siteOf(base);
  return (hosts ?? "").split(/\s+/).some((cloud) => cloud !== "" && (host === cloud || host.endsWith(`.${cloud}`)));
}

function fields(ctx: Ctx, language: Language) {
  const url = settingField(
    URL_SETTING,
    pick(language, "SonarQube address", "Dirección de SonarQube", "Endereço do SonarQube"),
    pick(
      language,
      "SonarQube Cloud: https://sonarcloud.io. SonarQube Server: the address you open in the browser, with the context path if there is one (https://sonar.company.com or https://company.com/sonar).",
      "SonarQube Cloud: https://sonarcloud.io. SonarQube Server: la dirección que abres en el navegador, con la ruta de contexto si la hay (https://sonar.empresa.com o https://empresa.com/sonar).",
      "SonarQube Cloud: https://sonarcloud.io. SonarQube Server: o endereço que você abre no navegador, com o caminho de contexto se houver (https://sonar.empresa.com ou https://empresa.com/sonar).",
    ),
    ctx.config.url,
  );
  const organization = settingField(
    ORGANIZATION,
    pick(language, "Organization (SonarQube Cloud only)", "Organización (solo SonarQube Cloud)", "Organização (só SonarQube Cloud)"),
    pick(
      language,
      "Leave it blank for SonarQube Server, which has no organizations.",
      "Déjala en blanco para SonarQube Server, que no tiene organizaciones.",
      "Deixe em branco no SonarQube Server, que não tem organizações.",
    ),
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
  const base = baseOf(setting(creds, URL_SETTING), ctx.config.url);
  const organization = isCloud(base, ctx.config.cloud) ? setting(creds, ORGANIZATION)?.trim() || undefined : undefined;
  return { ctx, token: creds.token, base, organization, scheme: "bearer" };
}

function headersOf(api: Api, scheme: Scheme): Record<string, string> {
  if (scheme === "bearer") return bearer(api.token);
  return { authorization: `Basic ${api.ctx.encoding.convert({ data: `${api.token}:`, from: "text", to: "base64" })}` };
}

function urlOf(api: Api, path: string, pairs: [string, string][]): string {
  return `${api.base}${path}${pairs.length === 0 ? "" : `?${query(pairs)}`}`;
}

function answer(api: Api, path: string, pairs: [string, string][]): Response {
  const url = urlOf(api, path, pairs);
  const first = send(api.ctx, { method: "GET", url, headers: headersOf(api, api.scheme) });
  if (api.scheme === "basic" || (first.status !== 401 && first.status !== 403)) return first;
  const second = send(api.ctx, { method: "GET", url, headers: headersOf(api, "basic") });
  if (second.status >= 400) return first;
  api.scheme = "basic";
  return second;
}

function explained(api: Api, path: string, raw: Response): Response {
  try {
    return checked(raw);
  } catch (error) {
    if (!(error instanceof ActionError)) throw error;
    const body = String(raw.body ?? "").replace(/\s+/g, " ").slice(0, 300);
    const why = raw.status === 404 && !body.trim().startsWith("{") ? `; check that ${api.base} is the SonarQube address, with the context path if there is one` : "";
    const told = new ActionError("permanent", `SonarQube answered ${raw.status} to GET ${api.base}${path}${why}: ${body}`);
    told.class = error.class;
    throw told;
  }
}

function read(api: Api, path: string, raw: Response) {
  if (raw.body === "") return null;
  try {
    return JSON.parse(raw.body);
  } catch {
    return fail("permanent", `GET ${api.base}${path} did not answer the SonarQube Web API (HTTP ${raw.status}, not JSON); check the address and its context path`);
  }
}

function get(api: Api, path: string, pairs: [string, string][]) {
  return read(api, path, explained(api, path, answer(api, path, pairs)));
}

function scoped(target: Target): [string, string][] {
  if (target.pullRequest) return [["pullRequest", target.pullRequest]];
  return target.branch ? [["branch", target.branch]] : [];
}

function needsOrganization(api: Api) {
  if (isCloud(api.base, api.ctx.config.cloud) && !api.organization) {
    fail("permanent", "SonarQube Cloud needs the organization; fill it in the connection (leave it blank only for SonarQube Server)");
  }
}

function projects(api: Api, wanted: string) {
  needsOrganization(api);
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

export function unknownMetrics(body: string): string[] {
  const found = /metric keys are not found: ([^"]+)/i.exec(body);
  return found ? found[1].split(",").map((key) => key.trim()).filter(Boolean) : [];
}

function measured(api: Api, target: Target, keys: string[]) {
  const path = "/api/measures/component";
  const raw = answer(api, path, [["component", target.project], ["metricKeys", keys.join(",")], ...scoped(target)]);
  const missing = raw.status === 404 ? unknownMetrics(raw.body) : [];
  const known = keys.filter((key) => !missing.includes(key));
  if (missing.length > 0 && known.length > 0 && known.length < keys.length) return measured(api, target, known);
  return read(api, path, explained(api, path, raw));
}

function measures(api: Api, target: Target): Record<string, string> {
  const found = measured(api, target, MEASURES);
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

export function issueKind(issue: any): { type: string; severity: string } {
  const impact = listIn(issue?.impacts)[0];
  const quality = String(impact?.softwareQuality ?? "");
  const type = TYPES[String(issue?.type ?? "")] ?? IMPACT_TYPES[quality] ?? "code_smell";
  const severity = String(issue?.severity ?? "").toLowerCase();
  if (SEVERITIES.includes(severity)) return { type, severity };
  return { type, severity: IMPACT_SEVERITIES[String(impact?.severity ?? "")] ?? "info" };
}

function issues(api: Api, target: Target, base: string) {
  const found = get(api, "/api/issues/search", [["componentKeys", target.project], ["resolved", "false"], ["ps", ISSUES], ...scoped(target)]);
  const listed = listIn(found?.issues).map((issue) => {
    const key = textOf(issue, ["key"]);
    return {
      key,
      ...issueKind(issue),
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
  const path = "/api/project_pull_requests/list";
  const raw = answer(api, path, [["project", target.project]]);
  if (raw.status === 400 || raw.status === 404) return undefined;
  const listed = listIn(read(api, path, explained(api, path, raw))?.pullRequests);
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
  const path = "/api/qualitygates/project_status";
  const raw = answer(api, path, [["projectKey", target.project], ...scoped(target)]);
  if (raw.status === 404) return { status: "pending", url };
  const status = read(api, path, explained(api, path, raw))?.projectStatus;
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

function direct(api: Api, path: string) {
  const raw = send(api.ctx, { method: "GET", url: urlOf(api, path, []), headers: headersOf(api, api.scheme) });
  return raw.status === 401 || raw.status === 403 ? undefined : read(api, path, explained(api, path, raw));
}

function loginWith(api: Api, scheme: Scheme): string | undefined {
  api.scheme = scheme;
  const validated = direct(api, "/api/authentication/validate");
  if (validated !== undefined && typeof validated?.valid !== "boolean") {
    fail("permanent", `GET ${api.base}/api/authentication/validate did not answer {"valid": ...}; check the address and its context path`);
  }
  if (validated?.valid !== true) return undefined;
  const current = direct(api, "/api/users/current");
  const login = textOf(current, ["login"]);
  return current?.isLoggedIn === false || login === "" ? undefined : login;
}

function signedIn(api: Api): string {
  const login = loginWith(api, "bearer") ?? loginWith(api, "basic");
  if (login !== undefined) return login;
  return fail("auth", `SonarQube at ${api.base} did not accept the token (tried Bearer and the token as the basic-auth login); generate a user token in My Account › Security`);
}

export function accounts({ creds }: { creds: Creds }, ctx: Ctx) {
  const api = apiOf(ctx, creds);
  needsOrganization(api);
  const login = signedIn(api);
  const host = api.base.replace(/^https?:\/\//i, "");
  const label = [login, api.organization ?? host].filter(Boolean).join(" · ");
  const settings: Record<string, unknown> = { [URL_SETTING]: api.base };
  if (api.organization) settings[ORGANIZATION] = api.organization;
  return [{ id: `sonarqube-${host}-${api.organization ?? ""}`, label: label || "SonarQube", settings }];
}
