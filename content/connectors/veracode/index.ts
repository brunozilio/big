import {
  type Creds,
  type Language,
  NO_PROJECT,
  RECONNECT,
  blocked,
  connect,
  fail,
  fetchJson,
  optionalText,
  pick,
  query,
  setting,
  settingField,
  textAt,
  tokenField,
  unknownAction,
} from "sdk/connectors/index.ts";
import { security } from "sdk/connectors/contracts.ts";
import { type Filter, chosen, filterOf } from "./findings.ts";
import { type Keys, header, readKeys } from "./sign.ts";

export const REGION = "region";
export const REGIONS: [string, string, string][] = [
  ["com", "api.veracode.com", ""],
  ["eu", "api.veracode.eu", " EU"],
  ["us", "api.veracode.us", " FedRAMP"],
];
const APPLICATIONS = "/appsec/v1/applications";
const APPLICATION_PAGE = "100";
const FINDING_PAGE = "500";
const MOST_PAGES = 10;

type Ctx = { config: { region: string } } & Record<string, any>;

type Api = { ctx: Ctx; keys: Keys; host: string; label: string; region: string; project?: string };

export function regionOf(entered: string, fallback: string): [string, string, string] | undefined {
  let bare = entered.trim().replace(/^(https:\/\/)+/, "").replace(/\/+$/, "").toLowerCase();
  if (bare.startsWith("api.veracode.")) bare = bare.slice("api.veracode.".length);
  if (bare === "") bare = fallback;
  return REGIONS.find(([region]) => region === bare);
}

const regionField = (ctx: Ctx, language: Language) =>
  settingField(REGION, pick(language, "Veracode region", "Región de Veracode", "Região do Veracode"), "", ctx.config.region);

export function provider({ language }: { language?: Language }, ctx: Ctx) {
  const fields = [regionField(ctx, language ?? "pt-BR"), tokenField("apiId", "API ID", "", false), tokenField("apiKey", "API key", "", true)];
  return {
    name: "Veracode",
    logo: "logo.svg",
    capabilities: ["security"],
    auth: { kind: "token", fields, form: "json" },
    actions: security(),
  };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(language, "Your Veracode account", "Tu cuenta de Veracode", "Sua conta Veracode"),
    group: "security",
    button: connect(language),
    site: pick(language, "Veracode", "de Veracode", "do Veracode"),
    choose_title: pick(language, "Veracode account", "Cuenta de Veracode", "Conta do Veracode"),
    hidden: false,
  };
}

function apiOf(ctx: Ctx, creds: Creds): Api {
  const keys = readKeys(creds.token) ?? fail("auth", RECONNECT);
  const entered = setting(creds, REGION) ?? "";
  const found = regionOf(entered, ctx.config.region);
  if (found === undefined) {
    return fail("permanent", `${JSON.stringify(entered)} is not a Veracode region; use ${REGIONS.map(([region]) => region).join(", ")}`);
  }
  const [region, host, suffix] = found;
  return { ctx, keys, host, label: `Veracode${suffix}`, region, project: setting(creds, "project") };
}

function get(api: Api, path: string, pairs: [string, string][]) {
  const pathAndQuery = pairs.length === 0 ? path : `${path}?${query(pairs)}`;
  const nonce: string = api.ctx.crypto.random({ bytes: 16, output: "hex" });
  const millis: number = api.ctx.clock.millis();
  const authorization =
    header(api.ctx, api.keys, { method: "GET", host: api.host, pathAndQuery }, millis, nonce) ??
    fail("auth", "the Veracode API key is not valid hex; generate a new one");
  return fetchJson(api.ctx, { method: "GET", url: `https://${api.host}${pathAndQuery}`, headers: { authorization } });
}

function projectOf(application: unknown) {
  const guid = textAt(application, ["guid"]);
  const name = textAt(application, ["profile", "name"]);
  return guid === "" ? undefined : { id: guid, key: guid, name: name === "" ? guid : name };
}

function lastPage(answer: any, page: number, fetched: number): boolean {
  const total = answer?.page?.total_pages;
  const pages = typeof total === "number" && Number.isInteger(total) && total >= 0 ? total : 0;
  return fetched === 0 || page + 1 >= pages;
}

export function applications(api: Api, name?: string) {
  const found: { id: string; key: string; name: string }[] = [];
  for (let page = 0; page < MOST_PAGES; page++) {
    const pairs: [string, string][] = [["page", String(page)], ["size", APPLICATION_PAGE]];
    if (name !== undefined) pairs.push(["name", name]);
    const answer = get(api, APPLICATIONS, pairs);
    const listed = Array.isArray(answer?._embedded?.applications) ? answer._embedded.applications : [];
    for (const application of listed) {
      const project = projectOf(application);
      if (project) found.push(project);
    }
    if (lastPage(answer, page, listed.length)) break;
  }
  return found;
}

function projects(api: Api, args: Record<string, unknown>) {
  const trimmed = optionalText(args, "query")?.trim();
  const wanted = trimmed === "" ? undefined : trimmed;
  const lowered = (wanted ?? "").toLowerCase();
  const matches = (project: { name: string; key: string }) =>
    lowered === "" || project.name.toLowerCase().includes(lowered) || project.key.toLowerCase().includes(lowered);
  return applications(api, wanted).filter(matches);
}

export const isGuid = (text: string) => /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(text);

function projectGuid(api: Api, args: Record<string, unknown>): string {
  const typed = optionalText(args, "project")?.trim();
  const given = (typed === "" ? undefined : typed) ?? api.project ?? blocked(NO_PROJECT);
  if (isGuid(given)) return given;
  const found = applications(api, given);
  const exact = found.filter((project) => project.name.toLowerCase() === given.toLowerCase());
  if (exact.length === 1) return exact[0].id;
  if (exact.length === 0 && found.length === 1) return found[0].id;
  if (exact.length === 0 && found.length === 0) return fail("permanent", `no Veracode application is named ${JSON.stringify(given)}`);
  return fail("permanent", `several Veracode applications match ${JSON.stringify(given)}; give its GUID`);
}

function scanFindings(api: Api, path: string, scan: string, filter: Filter) {
  const found: unknown[] = [];
  for (let page = 0; page < MOST_PAGES; page++) {
    const pairs: [string, string][] = [["size", FINDING_PAGE], ["page", String(page)], ["scan_type", scan]];
    if (filter.policy) pairs.push(["violates_policy", "TRUE"]);
    if (filter.minSeverity > 0) pairs.push(["severity_gte", String(filter.minSeverity)]);
    const answer = get(api, path, pairs);
    const listed = Array.isArray(answer?._embedded?.findings) ? answer._embedded.findings : [];
    found.push(...listed);
    if (lastPage(answer, page, listed.length)) break;
  }
  return found;
}

function securityFindings(api: Api, args: Record<string, unknown>) {
  const filter = filterOf(args);
  const guid = projectGuid(api, args);
  const path = `/appsec/v2/applications/${guid}/findings`;
  const raw: unknown[] = [];
  for (const scan of filter.scanTypes) raw.push(...scanFindings(api, path, scan, filter));
  return chosen(raw, filter);
}

export function call({ action, args, creds }: { action: string; args: Record<string, unknown>; creds: Creds }, ctx: Ctx) {
  const api = apiOf(ctx, creds);
  if (action === "security.projects") return projects(api, args);
  if (action === "security.findings") return securityFindings(api, args);
  return unknownAction(action);
}

export function accounts({ creds }: { creds: Creds }, ctx: Ctx) {
  const api = apiOf(ctx, creds);
  const found = applications(api);
  const settings: Record<string, unknown> = { [REGION]: api.region, shown: { projects: found.map((project) => project.name) } };
  if (found.length === 1) settings.project = found[0].key;
  return [{ id: `veracode-${api.region}`, label: api.label, settings }];
}
