import { fail, missing, listOf, optionalFlag, optionalNumber, textAt } from "sdk/connectors/index.ts";

export const KINDS: [string, string][] = [
  ["static", "STATIC"],
  ["dependency", "SCA"],
  ["dynamic", "DYNAMIC"],
  ["manual", "MANUAL"],
];
export const DEFAULT_LIMIT = 50;
export const MOST_LIMIT = 500;
const MOST_DESCRIPTION = 2000;
const ENTITIES: [string, string][] = [
  ["&lt;", "<"],
  ["&gt;", ">"],
  ["&quot;", '"'],
  ["&#39;", "'"],
  ["&apos;", "'"],
  ["&nbsp;", " "],
  ["&amp;", "&"],
];

export type Filter = { scanTypes: string[]; minSeverity: number; policy: boolean; open: boolean; limit: number };

function scanTypeOf(kind: string): string {
  const found = KINDS.find(([name]) => name.toLowerCase() === kind.trim().toLowerCase());
  if (found) return found[1];
  return fail("permanent", `${JSON.stringify(kind)} is not a kind of finding; use ${KINDS.map(([name]) => name).join(", ")}`);
}

export function filterOf(args: Record<string, unknown>): Filter {
  let scanTypes: string[] = [];
  for (const kind of listOf(args, "kinds")) {
    if (typeof kind !== "string") missing("kinds");
    const scan = scanTypeOf(kind as string);
    if (!scanTypes.includes(scan)) scanTypes.push(scan);
  }
  if (scanTypes.length === 0) scanTypes = KINDS.map(([, scan]) => scan);
  const limit = Math.min(Math.max(optionalNumber(args, "limit") ?? DEFAULT_LIMIT, 1), MOST_LIMIT);
  return {
    scanTypes,
    minSeverity: Math.min(optionalNumber(args, "minSeverity") ?? 0, 5),
    policy: optionalFlag(args, "policy") ?? false,
    open: optionalFlag(args, "open") ?? true,
    limit,
  };
}

const severityOf = (value: unknown) => (typeof value === "number" && Number.isInteger(value) && value >= 0 ? value : 0);

export function keeps(filter: Filter, finding: Record<string, unknown>): boolean {
  return (
    severityOf(finding.severity) >= filter.minSeverity &&
    (!filter.policy || finding.policy === true) &&
    (!filter.open || finding.status === "open")
  );
}

export function plain(html: string): string {
  let text = "";
  let inside = false;
  for (const c of html) {
    if (c === "<") {
      inside = true;
      text += " ";
    } else if (c === ">" && inside) inside = false;
    else if (!inside) text += c;
  }
  const decoded = ENTITIES.reduce((current, [entity, shown]) => current.split(entity).join(shown), text);
  const spaced = decoded.split(/\s+/).filter((word) => word !== "");
  return [...spaced.join(" ")].slice(0, MOST_DESCRIPTION).join("");
}

function wordOf(value: unknown): string | undefined {
  if (typeof value === "string" && value.trim() !== "") return value.trim();
  if (typeof value === "number") return String(value);
  return undefined;
}

const textOrNull = (value: unknown) => wordOf(value) ?? null;

const kindOf = (scan: string) => KINDS.find(([, known]) => known.toLowerCase() === scan.toLowerCase())?.[0] ?? "static";

function cweOf(details: any) {
  const id = wordOf(details?.cwe?.id);
  return id === undefined ? null : { id, name: textAt(details?.cwe, ["name"]) };
}

function cveOf(details: any) {
  const cve = details?.cve;
  const id = wordOf(cve?.name);
  if (id === undefined) return null;
  const score = cve?.cvss3?.score;
  const cvss = typeof score === "number" ? score : typeof cve?.cvss === "number" ? cve.cvss : null;
  return { id, cvss };
}

const isOpen = (status: unknown) =>
  textAt(status, ["status"]).toUpperCase() === "OPEN" && textAt(status, ["resolution_status"]).toUpperCase() !== "APPROVED";

function titleOf(kind: string, details: any, cwe: any, cve: any): string {
  const component = wordOf(details?.component_filename);
  const cweId = wordOf(cwe?.id);
  const weakness = wordOf(cwe?.name) ?? (cweId === undefined ? undefined : `CWE-${cweId}`);
  const named = kind === "dependency" ? (wordOf(cve?.id) ?? weakness) : weakness;
  if (named !== undefined && component !== undefined && kind === "dependency") return `${named} in ${component}`;
  if (named !== undefined) return named;
  if (component !== undefined) return component;
  return textAt(details, ["attack_vector"]);
}

const fileOf = (details: any) => textOrNull(details?.file_path) ?? textOrNull(details?.file_name);

export function normalized(found: any) {
  const details = found?.finding_details;
  const status = found?.finding_status;
  const kind = kindOf(textAt(found, ["scan_type"]));
  const cwe = cweOf(details);
  const cve = cveOf(details);
  const line = details?.file_line_number;
  return {
    id: wordOf(found?.issue_id) ?? "",
    title: titleOf(kind, details, cwe, cve),
    severity: Math.min(severityOf(details?.severity), 5),
    kind,
    status: isOpen(status) ? "open" : "closed",
    new: status?.new === true,
    policy: found?.violates_policy === true,
    cwe,
    cve,
    file: fileOf(details),
    line: typeof line === "number" && Number.isInteger(line) && line >= 0 ? line : null,
    module: textOrNull(details?.module),
    component: textOrNull(details?.component_filename),
    version: textOrNull(details?.version),
    description: plain(typeof found?.description === "string" ? found.description : ""),
    found: textOrNull(status?.first_found_date),
    url: textOrNull(details?.url),
  };
}

function bySeverity(a: any, b: any): number {
  const number = (finding: any) => (/^\d+$/.test(finding.id) ? Number(finding.id) : Number.MAX_SAFE_INTEGER);
  if (a.severity !== b.severity) return b.severity - a.severity;
  if (number(a) !== number(b)) return number(a) - number(b);
  return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
}

export function chosen(raw: unknown[], filter: Filter) {
  const kept = raw.map(normalized).filter((finding) => keeps(filter, finding));
  kept.sort(bySeverity);
  const unique = kept.filter((finding, at) => at === 0 || !(finding.id === kept[at - 1].id && finding.kind === kept[at - 1].kind));
  return unique.slice(0, filter.limit);
}
