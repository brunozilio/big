export type Schema = Record<string, unknown>;

export type Action = {
  name: string;
  input_schema: Schema;
  output_schema: Schema;
  writes: boolean;
  idempotent: "native" | "lookup";
};

type Fields = [string, string | string[]][];

export function object(required: Fields, optional: Fields = []): Schema {
  const properties: Record<string, Schema> = {};
  for (const [name, type] of [...required, ...optional]) properties[name] = { type };
  return { type: "object", properties, required: required.map(([name]) => name) };
}

export const list = (): Schema => ({ type: "array" });
const listOf = (items: Schema): Schema => ({ type: "array", items });

export const read = (name: string, input: Schema, output: Schema): Action => ({
  name,
  input_schema: input,
  output_schema: output,
  writes: false,
  idempotent: "native",
});

export const write = (name: string, input: Schema, output: Schema, idempotent: "native" | "lookup"): Action => ({
  name,
  input_schema: input,
  output_schema: output,
  writes: true,
  idempotent,
});

const issueOutput = () =>
  object(
    [["ref", "string"], ["title", "string"], ["status", "string"], ["url", "string"]],
    [
      ["id", "string"],
      ["project", "string"],
      ["type", "string"],
      ["parent", "string"],
      ["description", "string"],
      ["impediment", "object"],
      ["subtasks", "array"],
      ["attachments", "array"],
      ["comments", "array"],
    ],
  );

const issueSummary = () =>
  object([["ref", "string"], ["status", "string"]], [["id", "string"], ["title", "string"], ["url", "string"], ["changed", "boolean"]]);

const projectList = () => listOf(object([["id", "string"], ["key", "string"], ["name", "string"]], [["avatar", "string"]]));

const fieldList = () =>
  listOf(object([["key", "string"], ["name", "string"], ["type", "string"]], [["required", "boolean"], ["values", "array"]]));

const trackerSchema = () => {
  const status = object([["name", "string"]], [["id", "string"], ["category", "string"]]);
  const type = object([["name", "string"], ["level", "integer"], ["children", "array"], ["statuses", "array"]], [["id", "string"]]);
  const shaped = object([["types", "array"]], [["default", "string"]]);
  (shaped.properties as Record<string, Schema>).types = listOf({ ...type, properties: { ...(type.properties as object), statuses: listOf(status) } });
  return shaped;
};

export function tracker(): Action[] {
  const newIssue = object(
    [["title", "string"]],
    [["project", "string"], ["description", "string"], ["type", "string"], ["parent", "string"], ["labels", "array"], ["fields", "object"]],
  );
  const search = object([], [["query", "string"], ["project", "string"], ["status", "string"], ["limit", "integer"], ["cursor", "string"], ["pick", "boolean"]]);
  return [
    write("tracker.create_issue", newIssue, issueSummary(), "lookup"),
    read("tracker.issue", object([["issue", "string"]]), issueOutput()),
    write(
      "tracker.comment",
      object([["issue", "string"], ["body", "string"]]),
      object([["id", "string"], ["body", "string"]], [["created", "boolean"]]),
      "lookup",
    ),
    read("tracker.statuses", object([], [["project", "string"]]), list()),
    write("tracker.transition", object([["issue", "string"]], [["status", "string"], ["category", "string"]]), issueSummary(), "lookup"),
    read("tracker.schema", object([], [["project", "string"]]), trackerSchema()),
    write(
      "tracker.set_impediment",
      object([["issue", "string"], ["on", "boolean"]], [["reason", "string"]]),
      object([["on", "boolean"]], [["reason", "string"]]),
      "lookup",
    ),
    read("tracker.search", search, object([["issues", "array"]], [["cursor", "string"]])),
    read("tracker.projects", object([], [["query", "string"]]), projectList()),
    read("tracker.fields", object([], [["project", "string"], ["type", "string"]]), fieldList()),
    write(
      "tracker.update_issue",
      object([["issue", "string"]], [["title", "string"], ["description", "string"], ["fields", "object"]]),
      issueSummary(),
      "native",
    ),
    write(
      "tracker.link_issues",
      object([["issue", "string"], ["to", "string"]], [["kind", "string"]]),
      object([["issue", "string"], ["to", "string"]], [["already", "boolean"]]),
      "lookup",
    ),
    write("tracker.attach", object([["issue", "string"], ["files", "array"]]), object([["attached", "array"]], [["skipped", "array"]]), "lookup"),
  ];
}

const pullOutput = () =>
  object(
    [
      ["ref", "string"],
      ["repo", "string"],
      ["number", "integer"],
      ["state", "string"],
      ["url", "string"],
      ["head", "string"],
      ["base", "string"],
      ["title", "string"],
    ],
    [["body", "string"], ["created", "boolean"], ["checks", "array"], ["reviews", "array"], ["merged", "boolean"]],
  );

const pullRef = () => object([["pr", "string"]]);

export function code(): Action[] {
  const open = object([["head", "string"], ["title", "string"]], [["repo", "string"], ["base", "string"], ["body", "string"], ["draft", "boolean"]]);
  const review = object([["pr", "string"], ["verdict", "string"]], [["body", "string"], ["lines", "array"], ["agent", "string"]]);
  const merge = object([["pr", "string"]], [["method", "string"], ["delete_branch", "boolean"]]);
  const repo = object([["id", "string"], ["ref", "string"], ["name", "string"]], [["url", "string"], ["default_branch", "string"], ["private", "boolean"]]);
  return [
    write("code.open_pull_request", open, pullOutput(), "lookup"),
    read("code.pull_request", pullRef(), pullOutput()),
    read("code.files_diff", pullRef(), list()),
    read("code.review_comments", pullRef(), list()),
    write("code.submit_review", review, object([["id", "string"], ["verdict", "string"]], [["created", "boolean"]]), "lookup"),
    write(
      "code.resolve_thread",
      object([["thread", "string"]], [["pr", "string"]]),
      object([["thread", "string"], ["resolved", "boolean"]]),
      "native",
    ),
    write("code.merge", merge, object([["merged", "boolean"]], [["sha", "string"], ["already", "boolean"], ["deleted", "boolean"]]), "lookup"),
    read("code.repos", object([], [["query", "string"]]), listOf(repo)),
    read("code.pull_requests", object([], [["repo", "string"]]), list()),
    read("code.branches", object([], [["repo", "string"]]), listOf(object([["name", "string"]], [["default", "boolean"]]))),
    write(
      "code.merge_branches",
      object([["base", "string"], ["head", "string"]], [["repo", "string"], ["message", "string"]]),
      object([["merged", "boolean"]], [["sha", "string"], ["already", "boolean"]]),
      "lookup",
    ),
  ];
}

export function pipelines(): Action[] {
  const runRef = object([["run", "string"]]);
  const runOutput = object([["ref", "string"], ["state", "string"]], [["pipeline", "string"], ["head", "string"], ["url", "string"], ["jobs", "array"]]);
  const runs = object([], [["ref", "string"], ["repo", "string"], ["pipeline", "string"], ["limit", "integer"], ["status", "string"]]);
  const review = object([["run", "string"], ["decision", "string"]], [["deployments", "array"], ["comment", "string"]]);
  return [
    read("pipelines.runs", runs, list()),
    read("pipelines.run", runRef, runOutput),
    read("pipelines.job_log", object([["run", "string"], ["job", "string"]], [["tail", "integer"]]), object([["text", "string"]], [["truncated", "boolean"]])),
    read("pipelines.pending_deployments", runRef, list()),
    write("pipelines.review_deployment", review, object([["decision", "string"], ["deployments", "array"]], [["already", "boolean"]]), "lookup"),
  ];
}

export function cloud(): Action[] {
  const credentials = object(
    [["account", "string"], ["access_key_id", "string"], ["secret_access_key", "string"]],
    [["role", "string"], ["arn", "string"], ["session_token", "string"], ["expires_at", "string"], ["region", "string"]],
  );
  return [
    read("cloud.identity", object([], [["account", "string"], ["role", "string"], ["binding", "string"]]), credentials),
    read("cloud.accounts_and_roles", { type: "object", properties: {} }, object([["accounts", "array"]])),
    read(
      "cloud.last_deployment",
      object([["service", "string"], ["environment", "string"]], [["account", "string"]]),
      object([["version", "string"]], [["at", "string"], ["source", "string"]]),
    ),
  ];
}

export function changes(): Action[] {
  const fields: Fields = [
    ["settings", "object"],
    ["tasks", "array"],
    ["notes", "array"],
    ["attachments", "array"],
    ["versions", "array"],
    ["description", "string"],
  ];
  const output = object(
    [["ref", "string"], ["key", "string"], ["status", "string"]],
    [["title", "string"], ["environment", "string"], ["start", "string"], ["end", "string"], ["url", "string"], ["revision", "integer"], ["created", "boolean"]],
  );
  const changeRef = object([["change", "string"]]);
  return [
    write("changes.create", object([["title", "string"], ["environment", "string"], ["start", "string"], ["end", "string"]], fields), output, "lookup"),
    write("changes.update", object([["change", "string"]], [["title", "string"], ["start", "string"], ["end", "string"], ...fields]), output, "native"),
    read("changes.get", changeRef, output),
    write("changes.cancel", changeRef, output, "native"),
    options("changes"),
  ];
}

function withIssueId(schema: Schema): Schema {
  (schema.properties as Record<string, Schema>).issue_id = { type: ["string", "integer"] };
  return schema;
}

export function tests(): Action[] {
  const newCase = withIssueId(
    object(
      [["title", "string"]],
      [
        ["project", "string"],
        ["objective", "string"],
        ["precondition", "string"],
        ["labels", "array"],
        ["steps", "array"],
        ["folder", "string"],
        ["issue", "string"],
        ["issue_id", "string"],
      ],
    ),
  );
  const newCycle = withIssueId(object([["title", "string"]], [["project", "string"], ["description", "string"], ["issue", "string"], ["issue_id", "string"]]));
  const record = withIssueId(
    object([["case", "string"], ["cycle", "string"], ["status", "string"]], [["project", "string"], ["comment", "string"], ["seconds", "number"], ["issue_id", "string"]]),
  );
  const finish = object([["cycle", "string"], ["status", "string"]], [["project", "string"]]);
  return [
    write("tests.create_case", newCase, object([["ref", "string"], ["title", "string"], ["url", "string"]], [["created", "boolean"]]), "lookup"),
    read("tests.cases", object([["issue", "string"]]), listOf(object([["ref", "string"], ["title", "string"]]))),
    write("tests.create_cycle", newCycle, object([["ref", "string"], ["url", "string"]]), "native"),
    write("tests.record", record, object([["ref", "string"]]), "native"),
    write("tests.finish_cycle", finish, object([["ref", "string"]], [["status", "string"]]), "native"),
    read("tests.projects", object([], [["query", "string"]]), projectList()),
  ];
}

export function securityStatus(): Action {
  return read("security.status", object([], [["project", "string"], ["since", "string"]]), object([["status", "string"]], [["scannedAt", ["string", "null"]], ["running", "boolean"]]));
}

export function security(): Action[] {
  const finding = object(
    [
      ["id", "string"],
      ["title", "string"],
      ["severity", "integer"],
      ["kind", "string"],
      ["status", "string"],
      ["new", "boolean"],
      ["policy", "boolean"],
      ["description", "string"],
    ],
    [
      ["cwe", "object"],
      ["cve", "object"],
      ["file", "string"],
      ["line", "integer"],
      ["module", "string"],
      ["component", "string"],
      ["version", "string"],
      ["found", "string"],
      ["url", "string"],
    ],
  );
  const wanted = object([], [["project", "string"], ["kinds", "array"], ["minSeverity", "integer"], ["policy", "boolean"], ["open", "boolean"], ["limit", "integer"]]);
  return [read("security.projects", object([], [["query", "string"]]), projectList()), read("security.findings", wanted, listOf(finding))];
}

export function mcp(): Action[] {
  return [read("mcp.tools", object([]), list())];
}

export function quality(): Action[] {
  const wanted = object([["project", "string"]], [["pullRequest", "string"], ["branch", "string"], ["commit", "string"]]);
  const report = object(
    [["status", "string"]],
    [
      ["gate", "string"],
      ["ratings", "object"],
      ["counts", "object"],
      ["coverage", ["number", "null"]],
      ["duplication", ["number", "null"]],
      ["conditions", "array"],
      ["issues", "array"],
      ["total", "integer"],
      ["url", "string"],
      ["analyzedAt", ["string", "null"]],
    ],
  );
  return [read("quality.report", wanted, report)];
}

export type BindingScope = "application" | "repository";

export type BindingField = { key: string; label: string; required?: boolean };

export type BindingEach = { key: string; label: string; fields: BindingField[] };

export type BindingSpec = { capability: string; scope: BindingScope; label: string; multiple?: boolean; fields?: BindingField[]; each?: BindingEach };

export function binding(capability: string, scope: BindingScope, label: string, shape: { multiple?: boolean; fields?: BindingField[]; each?: BindingEach } = {}): BindingSpec {
  return { capability, scope, label, ...shape };
}

export function bindingField(key: string, label: string, required = false): BindingField {
  return { key, label, required };
}

export function options(capability: string): Action {
  const option = object([["value", "string"], ["label", "string"]], [["detail", "string"]]);
  return read(`${capability}.options`, object([], [["query", "string"], ["field", "string"]]), listOf(option));
}

export function projectOptions(found: unknown): { value: string; label: string }[] {
  const listed = Array.isArray(found) ? found : [];
  return listed
    .map((project) => ({ key: String(project?.key ?? ""), name: String(project?.name ?? "") }))
    .filter((project) => project.key !== "")
    .map((project) => ({ value: project.key, label: project.name && project.name !== project.key ? `${project.name} · ${project.key}` : project.key }));
}
