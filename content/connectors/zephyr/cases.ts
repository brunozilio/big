import { NO_PROJECT, blocked, decode, fail, listOf, optionalNumber, optionalText, text, textAt } from "sdk/connectors/index.ts";
import { items } from "sdk/connectors/mcp.ts";
import { carriesTag, label, tagged } from "sdk/connectors/idempotency.ts";
import { type Api, MOST_PAGES, PAGE, idOf, lastPage, projectFor, validKey } from "./api.ts";

const LOOKUP_PAGE = 1000;
const CASE_FOLDERS = "TEST_CASE";
const STATUSES: [string, string][] = [
  ["passed", "Pass"],
  ["failed", "Fail"],
  ["blocked", "Blocked"],
  ["not_run", "Not Executed"],
];

const words = (text: string) => text.split(/\s+/).filter((word) => word !== "").join(" ").toLowerCase();
const sameTitle = (left: string, right: string) => words(left) === words(right);

function issueId(args: Record<string, unknown>): number | undefined {
  const value = args?.issue_id;
  if (value === undefined || value === null || value === "") return undefined;
  return optionalNumber(args, "issue_id") ?? fail("permanent", "`issue_id` is the numeric id of the issue");
}

function linked(api: Api, issue: string) {
  const answer = api.tool("zephyr_get_issue_link_test_cases", { issueKey: validKey(issue) });
  const found: { ref: string; title: string }[] = [];
  for (const link of items(answer, "values")) {
    const key = textAt(link, ["key"]);
    if (key === "") continue;
    const testCase = api.tool("zephyr_get_test_case", { testCaseKey: key });
    found.push({ ref: key, title: textAt(testCase, ["name"]) });
  }
  return found;
}

export const cases = (api: Api, issue: string) => linked(api, issue);

function stepOf(step: unknown) {
  if (typeof step === "string") return { inline: { description: step, expectedResult: "" } };
  const inline: Record<string, unknown> = { description: textAt(step, ["action"]) };
  const data = (step as any)?.data;
  if (typeof data === "string" && data !== "") inline.testData = data;
  inline.expectedResult = textAt(step, ["expected"]);
  return { inline };
}

function existingFolder(api: Api, project: string, name: string) {
  for (let page = 0; page < MOST_PAGES; page++) {
    const wanted = { projectKey: project, folderType: CASE_FOLDERS, startAt: page * PAGE, maxResults: PAGE };
    const answer = api.tool("zephyr_get_folders", wanted);
    const folders = items(answer, "values");
    const found = folders.find((folder) => (folder?.parentId ?? null) === null && sameTitle(textAt(folder, ["name"]), name));
    if (found) return found;
    if (lastPage(answer, folders.length)) break;
  }
  return undefined;
}

function folderId(api: Api, project: string, name: string) {
  const folder = existingFolder(api, project, name) ?? api.tool("zephyr_create_folder", { projectKey: project, name, folderType: CASE_FOLDERS });
  const id = folder?.id;
  if (typeof id === "number") return id;
  if (typeof id === "string") return /^\d+$/.test(id) ? Number(id) : id;
  return decode("the test folder has no id");
}

function withMarker(api: Api, body: string | undefined, key: string): string | undefined {
  if (key !== "") return tagged(api.ctx, body ?? "", key);
  return body;
}

function reference(found: unknown): string {
  const key = textAt(found, ["key"]);
  return key === "" ? idOf(found, ["id"]) : key;
}

function byCursor(api: Api, tool: string, wanted: Record<string, unknown>, matches: (value: any) => boolean) {
  for (let turn = 0; turn < MOST_PAGES; turn++) {
    const answer = api.tool(tool, { ...wanted });
    const values = items(answer, "values");
    const found = values.find(matches);
    if (found) return found;
    const next = answer?.nextStartAtId;
    if (typeof next === "number" && Number.isInteger(next) && next >= 0 && values.length > 0) wanted.startAtId = next;
    else break;
  }
  return undefined;
}

function keyedCase(api: Api, project: string, title: string, key: string) {
  if (key === "") return undefined;
  const mark = label(api.ctx, key);
  const wanted = { projectKey: project, nameOrKey: title, limit: PAGE };
  return byCursor(api, "zephyr_get_test_cases", wanted, (found) => Array.isArray(found?.labels) && found.labels.includes(mark));
}

function keyedCycle(api: Api, project: string, key: string) {
  if (key === "") return undefined;
  let start = 0;
  for (let turn = 0; turn < MOST_PAGES; turn++) {
    const answer = api.tool("zephyr_get_test_cycles", { projectKey: project, maxResults: LOOKUP_PAGE, startAt: start });
    const cycles = items(answer, "values");
    const found = cycles.find((cycle) => carriesTag(api.ctx, textAt(cycle, ["description"]), key));
    if (found) return found;
    const fetched = cycles.length;
    const done = typeof answer?.isLast === "boolean" ? answer.isLast || fetched === 0 : fetched < LOOKUP_PAGE;
    if (done) break;
    start += fetched;
  }
  return undefined;
}

function newCase(api: Api, project: string, title: string, args: Record<string, unknown>, key: string) {
  const wanted: Record<string, unknown> = { projectKey: project, name: title };
  const objective = optionalText(args, "objective");
  if (objective !== undefined) wanted.objective = objective;
  const precondition = optionalText(args, "precondition");
  if (precondition !== undefined) wanted.precondition = precondition;
  const labels = listOf(args, "labels").filter((item) => typeof item === "string") as string[];
  if (key !== "") labels.push(label(api.ctx, key));
  if (labels.length > 0) wanted.labels = labels;
  return wanted;
}

export function createCase(api: Api, args: Record<string, unknown>, key: string) {
  const title = text(args, "title");
  const link = issueId(args);
  const issue = optionalText(args, "issue");
  if (issue !== undefined) {
    const found = linked(api, issue).find((existing) => sameTitle(existing.title, title));
    if (found) return { ref: found.ref, title: found.title, url: "", created: false };
  }
  const project = projectFor(api, args);
  const existing = keyedCase(api, project, title, key);
  if (existing) return { ref: reference(existing), title: textAt(existing, ["name"]), url: "", created: false };
  const wanted = newCase(api, project, title, args, key);
  const folder = optionalText(args, "folder")?.trim();
  if (folder) wanted.folderId = folderId(api, project, folder);
  const made = api.tool("zephyr_create_test_case", wanted);
  const created = textAt(made, ["key"]);
  if (created === "") decode("the new test case has no key");
  const steps = listOf(args, "steps").map(stepOf);
  if (steps.length > 0) api.tool("zephyr_create_test_case_steps", { testCaseKey: created, mode: "OVERWRITE", items: steps });
  if (link !== undefined) api.tool("zephyr_create_test_case_issue_link", { testCaseKey: created, issueId: link });
  return { ref: created, title, url: "", created: true };
}

export function createCycle(api: Api, args: Record<string, unknown>, key: string) {
  const title = text(args, "title");
  const link = issueId(args);
  const project = projectFor(api, args);
  const existing = keyedCycle(api, project, key);
  if (existing) return { ref: reference(existing), url: "" };
  const wanted: Record<string, unknown> = { projectKey: project, name: title };
  const description = withMarker(api, optionalText(args, "description"), key);
  if (description !== undefined) wanted.description = description;
  const made = api.tool("zephyr_create_test_cycle", wanted);
  const cycle = textAt(made, ["key"]);
  if (cycle === "") decode("the new test cycle has no key");
  if (link !== undefined) api.tool("zephyr_create_test_cycle_issue_link", { testCycleIdOrKey: cycle, issueId: link });
  return { ref: cycle, url: "" };
}

function statusName(status: string): string {
  const found = STATUSES.find(([name]) => name.toLowerCase() === status.trim().toLowerCase());
  if (found) return found[1];
  return fail("permanent", `${JSON.stringify(status)} is not a test status; use one of ${STATUSES.map(([name]) => name).join(", ")}`);
}

function caseProject(testCase: string): string | undefined {
  const at = testCase.lastIndexOf("-T");
  return at > 0 ? testCase.slice(0, at) : undefined;
}

export function record(api: Api, args: Record<string, unknown>, key: string) {
  const testCase = validKey(text(args, "case"));
  const cycle = validKey(text(args, "cycle"));
  const status = statusName(text(args, "status"));
  const project = validKey(optionalText(args, "project") ?? api.project ?? caseProject(testCase) ?? blocked(NO_PROJECT));
  if (key !== "") {
    const wanted = { projectKey: project, testCycle: cycle, testCase, limit: PAGE };
    const found = byCursor(api, "zephyr_get_test_executions", wanted, (execution) => carriesTag(api.ctx, textAt(execution, ["comment"]), key));
    if (found) return { ref: reference(found) };
  }
  const wanted: Record<string, unknown> = { projectKey: project, testCaseKey: testCase, testCycleKey: cycle, statusName: status };
  const comment = withMarker(api, optionalText(args, "comment"), key);
  if (comment !== undefined) wanted.comment = comment;
  const seconds = args?.seconds;
  if (typeof seconds === "number" && seconds >= 0) wanted.executionTime = Math.round(seconds * 1000);
  return { ref: reference(api.tool("zephyr_create_test_execution", wanted)) };
}
