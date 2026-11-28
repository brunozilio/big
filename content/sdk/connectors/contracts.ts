export type Schema = Record<string, unknown>;

export type Effect = "read" | "write" | "send-external" | "money" | "pii" | "legal" | "destructive" | "memory" | "consumes-budget";

export type Risk = "low" | "medium" | "high";

export type Reversible = boolean | "compensate";

export type Visibility = "model" | "host";

export type Permission = { default: "ask" | "deny"; alwaysAsk: boolean };

export type Profile = {
  effects: Effect[];
  risk: Risk;
  reversible: Reversible;
  untrusted: boolean;
  sensitive: boolean;
  recipients?: string[];
  freeText?: string[];
  visibility?: Visibility;
  permission?: Permission;
};

export type Action = {
  name: string;
  input_schema: Schema;
  output_schema: Schema;
  writes: boolean;
  idempotent: "native" | "lookup";
  description?: string;
  secret?: boolean;
} & Profile;

export const looks = (untrusted = false, sensitive = false, more: Effect[] = []): Profile => ({
  effects: ["read", ...more],
  risk: "low",
  reversible: true,
  untrusted,
  sensitive,
});

export const alters = (risk: Risk, reversible: Reversible, more: Partial<Profile> = {}): Profile => ({
  effects: ["write"],
  risk,
  reversible,
  untrusted: false,
  sensitive: false,
  ...more,
});

const ALWAYS_ASK: Permission = { default: "ask", alwaysAsk: true };

const PROFILES: Record<string, Profile> = {
  "tracker.create_issue": alters("low", "compensate"),
  "tracker.issue": looks(true),
  "tracker.comment": alters("low", "compensate", { freeText: ["body"] }),
  "tracker.statuses": looks(),
  "tracker.transition": alters("low", true),
  "tracker.schema": looks(),
  "tracker.set_impediment": alters("low", true, { freeText: ["reason"] }),
  "tracker.search": looks(true),
  "tracker.projects": looks(),
  "tracker.fields": looks(),
  "tracker.update_issue": alters("medium", "compensate", { freeText: ["title", "description"] }),
  "tracker.link_issues": alters("low", true),
  "tracker.attach": alters("medium", "compensate", { sensitive: true }),
  "code.open_pull_request": alters("low", "compensate", { freeText: ["title", "body"] }),
  "code.pull_request": looks(true),
  "code.files_diff": looks(true),
  "code.review_comments": looks(true),
  "code.submit_review": alters("medium", "compensate", { freeText: ["body"] }),
  "code.resolve_thread": alters("low", true),
  "code.merge": alters("medium", "compensate"),
  "code.repos": looks(),
  "code.pull_requests": looks(true),
  "code.branches": looks(),
  "code.merge_branches": alters("medium", "compensate"),
  "code.git_credentials": { ...looks(false, true), visibility: "host" },
  "pipelines.runs": looks(),
  "pipelines.run": looks(),
  "pipelines.job_log": looks(true),
  "pipelines.pending_deployments": looks(),
  "pipelines.review_deployment": alters("medium", "compensate", { freeText: ["comment"] }),
  "cloud.identity": { ...looks(false, true), visibility: "host" },
  "cloud.accounts_and_roles": looks(),
  "cloud.last_deployment": looks(),
  "changes.create": alters("medium", "compensate", { freeText: ["title", "description"] }),
  "changes.update": alters("medium", true, { freeText: ["title", "description"] }),
  "changes.get": looks(),
  "changes.cancel": alters("medium", false),
  "tests.create_case": alters("low", "compensate"),
  "tests.cases": looks(),
  "tests.create_cycle": alters("low", "compensate"),
  "tests.record": alters("low", true),
  "tests.finish_cycle": alters("low", true),
  "tests.projects": looks(),
  "security.status": looks(),
  "security.projects": looks(),
  "security.findings": looks(false, true),
  "mcp.tools": looks(true),
  "quality.report": looks(),
  "email.list_messages": looks(true, true, ["pii"]),
  "email.message": looks(true, true, ["pii"]),
  "email.send": alters("high", false, { effects: ["send-external", "pii"], sensitive: true, recipients: ["to", "cc"], freeText: ["subject", "body"], permission: ALWAYS_ASK }),
  "email.reply": alters("high", false, { effects: ["send-external", "pii"], sensitive: true, freeText: ["body"], permission: ALWAYS_ASK }),
  "email.list_events": looks(true, true, ["pii"]),
  "email.create_event": alters("medium", "compensate", { effects: ["write", "send-external", "pii"], sensitive: true, recipients: ["attendees"], freeText: ["subject", "body"] }),
  "files.list": looks(false, true),
  "files.search": looks(true, true),
  "files.download": looks(true, true),
  "files.sites": looks(),
  "messaging.list_chats": looks(false, true, ["pii"]),
  "messaging.list_messages": looks(true, true, ["pii"]),
  "messaging.send_message": alters("medium", "compensate", { recipients: ["chat"], freeText: ["text"] }),
  "design.frame": looks(true),
  "assistant.ask": { ...looks(true, false, ["consumes-budget"]), freeText: ["question", "context"] },
};

export const CONNECTOR_SCHEMA = 2;

const legacy = (writes: boolean): Profile =>
  writes ? alters("medium", false, { untrusted: true, sensitive: true }) : looks(true, true);

export function migrated(manifest: { schema_version?: number; actions?: Partial<Action>[] }) {
  if (!manifest || typeof manifest !== "object" || (manifest.schema_version ?? 0) >= CONNECTOR_SCHEMA) return manifest;
  const actions = (manifest.actions ?? []).map((action) => {
    if (!action || typeof action !== "object" || action.effects !== undefined) return action;
    const hidden = action.secret === true ? { visibility: "host" as Visibility } : {};
    return { ...legacy(action.writes === true), ...hidden, ...action };
  });
  return { ...manifest, actions };
}


type Fields = [string, string | string[]][];

export function object(required: Fields, optional: Fields = []): Schema {
  const properties: Record<string, Schema> = {};
  for (const [name, type] of [...required, ...optional]) properties[name] = { type };
  return { type: "object", properties, required: required.map(([name]) => name) };
}

export const list = (): Schema => ({ type: "array" });
const listOf = (items: Schema): Schema => ({ type: "array", items });

const DESCRIPTIONS: Record<string, string> = {
  "tracker.create_issue": "Create an issue in the tracker and return its key and link.",
  "tracker.issue": "Read one issue: title, status, description and link.",
  "tracker.comment": "Add a comment to an issue.",
  "tracker.statuses": "List the statuses an issue can be in, for a project.",
  "tracker.transition": "Move an issue to another status.",
  "tracker.schema": "Describe the tracker's issue types and fields for a project.",
  "tracker.set_impediment": "Flag an issue as blocked, or clear the flag, with a reason.",
  "tracker.search": "Search issues by text, project or status; page with the cursor.",
  "tracker.projects": "List the tracker's projects, optionally matching a query.",
  "tracker.fields": "List the fields an issue type has in a project.",
  "tracker.update_issue": "Change an issue's title, description or fields.",
  "tracker.link_issues": "Link one issue to another.",
  "tracker.attach": "Attach files to an issue.",
  "code.open_pull_request": "Open a pull request from a branch.",
  "code.pull_request": "Read a pull request: title, state, branches and link.",
  "code.files_diff": "List the files a pull request changes, with their diffs.",
  "code.review_comments": "List the review comments on a pull request.",
  "code.submit_review": "Approve a pull request, request changes, or comment on it.",
  "code.resolve_thread": "Mark a review thread as resolved.",
  "code.merge": "Merge a pull request.",
  "code.repos": "List the repositories the connection can see, optionally matching a query.",
  "code.pull_requests": "List the open pull requests of a repository.",
  "code.branches": "List a repository's branches.",
  "code.merge_branches": "Merge one branch into another.",
  "code.git_credentials": "Give git credentials for a repository address.",
  "pipelines.runs": "List the pipeline runs, newest first.",
  "pipelines.run": "Read one pipeline run with its jobs and their states.",
  "pipelines.job_log": "Read the log of one job of a run.",
  "pipelines.pending_deployments": "List the deployments of a run waiting for approval.",
  "pipelines.review_deployment": "Approve or reject a run's pending deployments.",
  "cloud.identity": "Give cloud credentials for an account and role.",
  "cloud.accounts_and_roles": "List the cloud accounts and the roles the person can take in each.",
  "cloud.last_deployment": "Tell which version of a service is deployed in an environment, and when.",
  "changes.create": "Open a change request for a deploy window.",
  "changes.update": "Change a change request's fields.",
  "changes.get": "Read a change request and its status.",
  "changes.cancel": "Cancel a change request.",
  "tests.create_case": "Create a test case linked to an issue.",
  "tests.cases": "List the test cases linked to an issue.",
  "tests.create_cycle": "Create a test cycle.",
  "tests.record": "Record a test case's result in a cycle.",
  "tests.finish_cycle": "Close a test cycle.",
  "tests.projects": "List the test projects, optionally matching a query.",
  "security.status": "Tell whether a project's last security scan passed, and when it ran.",
  "security.projects": "List the security projects, optionally matching a query.",
  "security.findings": "List a project's security findings.",
  "mcp.tools": "List the tools the connected MCP server offers.",
  "quality.report": "Read a project's code quality report.",
  "email.list_messages": "List email messages in a folder, optionally matching a query.",
  "email.message": "Read one email message.",
  "email.send": "Send an email.",
  "email.reply": "Reply to an email message.",
  "email.list_events": "List calendar events between two moments.",
  "email.create_event": "Create a calendar event.",
  "files.list": "List the files in a folder.",
  "files.search": "Search files by text.",
  "files.download": "Read a file's content.",
  "files.sites": "List the sites whose files the connection can reach.",
  "messaging.list_chats": "List the chats the person is in.",
  "messaging.list_messages": "List the latest messages of a chat.",
  "messaging.send_message": "Send a message to a chat.",
  "design.frame": "Read a design frame by its link.",
  "assistant.ask": "Ask the connected assistant a question.",
};

type Plain = Omit<Action, keyof Profile>;

function described(action: Plain, given?: Profile): Action {
  const description = action.description ?? DESCRIPTIONS[action.name];
  const shown = description ? { ...action, description } : action;
  const profile = given ?? PROFILES[action.name];
  if (!profile) throw new TypeError(`${action.name} declares no profile: effects, risk, reversible, untrusted and sensitive`);
  return profile.visibility === "host" ? { ...shown, ...profile, secret: true } : { ...shown, ...profile };
}

export const read = (name: string, input: Schema, output: Schema, profile?: Profile): Action =>
  described({ name, input_schema: input, output_schema: output, writes: false, idempotent: "native" }, profile);

export const write = (name: string, input: Schema, output: Schema, idempotent: "native" | "lookup", profile?: Profile): Action =>
  described({ name, input_schema: input, output_schema: output, writes: true, idempotent }, profile);

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
    read("code.git_credentials", object([["url", "string"]]), object([["username", "string"], ["password", "string"]])),
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
    read("cloud.identity", object([], [["account", "string"], ["role", "string"], ["binding", "string"], ["profiles", "array"], ["environment", "string"]]), credentials),
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

export function email(): Action[] {
  const message = object(
    [["ref", "string"], ["subject", "string"], ["from", "string"], ["received", "string"]],
    [["to", "array"], ["preview", "string"], ["body", "string"], ["unread", "boolean"], ["url", "string"]],
  );
  const event = object(
    [["ref", "string"], ["subject", "string"], ["start", "string"], ["end", "string"]],
    [["location", "string"], ["organizer", "string"], ["attendees", "array"], ["online", "string"], ["url", "string"]],
  );
  const draft = object([["to", "array"], ["subject", "string"], ["body", "string"]], [["cc", "array"], ["html", "boolean"]]);
  const newEvent = object(
    [["subject", "string"], ["start", "string"], ["end", "string"]],
    [["attendees", "array"], ["location", "string"], ["body", "string"], ["online", "boolean"]],
  );
  return [
    read("email.list_messages", object([], [["folder", "string"], ["query", "string"], ["limit", "integer"], ["unread", "boolean"]]), listOf(message)),
    read("email.message", object([["message", "string"]]), message),
    write("email.send", draft, object([["sent", "boolean"]], [["ref", "string"]]), "native"),
    write("email.reply", object([["message", "string"], ["body", "string"]], [["all", "boolean"], ["html", "boolean"]]), object([["sent", "boolean"]]), "native"),
    read("email.list_events", object([], [["from", "string"], ["to", "string"], ["limit", "integer"]]), listOf(event)),
    write("email.create_event", newEvent, object([["ref", "string"]], [["url", "string"], ["online", "string"], ["created", "boolean"]]), "lookup"),
  ];
}

export function files(): Action[] {
  const item = object(
    [["ref", "string"], ["name", "string"], ["kind", "string"]],
    [["size", "integer"], ["modified", "string"], ["url", "string"], ["path", "string"], ["mime", "string"]],
  );
  return [
    read("files.list", object([], [["folder", "string"], ["drive", "string"], ["limit", "integer"]]), listOf(item)),
    read("files.search", object([["query", "string"]], [["drive", "string"], ["limit", "integer"]]), listOf(item)),
    read("files.download", object([["file", "string"]], [["drive", "string"]]), object([["name", "string"], ["mime", "string"]], [["text", "string"], ["base64", "string"], ["truncated", "boolean"]])),
    read("files.sites", object([], [["query", "string"]]), listOf(object([["ref", "string"], ["name", "string"]], [["url", "string"], ["host", "string"]]))),
  ];
}

export function messaging(): Action[] {
  const chat = object([["ref", "string"], ["kind", "string"]], [["topic", "string"], ["members", "array"], ["updated", "string"], ["url", "string"]]);
  const message = object([["ref", "string"], ["from", "string"], ["sent", "string"]], [["text", "string"], ["url", "string"]]);
  return [
    read("messaging.list_chats", object([], [["limit", "integer"]]), listOf(chat)),
    read("messaging.list_messages", object([["chat", "string"]], [["limit", "integer"]]), listOf(message)),
    write("messaging.send_message", object([["chat", "string"], ["text", "string"]], [["html", "boolean"]]), object([["sent", "boolean"]], [["ref", "string"]]), "native"),
  ];
}

export function design(): Action[] {
  const frame = object(
    [["ref", "string"], ["name", "string"]],
    [
      ["file", "string"],
      ["url", "string"],
      ["texts", "array"],
      ["outline", "string"],
      ["image", ["string", "null"]],
      ["picture", ["object", "null"]],
    ],
  );
  return [read("design.frame", object([["url", "string"]]), frame)];
}

export function assistant(): Action[] {
  const asked = object([["question", "string"]], [["context", "string"], ["thread", "string"], ["mode", "string"]]);
  const answered = object([["answer", "string"], ["thread", "string"], ["sources", "array"]], [["model", "string"]]);
  return [read("assistant.ask", asked, answered)];
}

export type BindingScope = "application" | "repository";

export type BindingField = { key: string; label: string; required?: boolean; skippable?: boolean };

export type BindingEach = { key: string; label: string; fields: BindingField[] };

export type BindingShape = { multiple?: boolean; fields?: BindingField[]; each?: BindingEach; section?: string; order?: number; suggests?: boolean; summary?: BindingSummary };

export type BindingSummary = { parts: string[]; separator: string };

export type BindingSpec = { capability: string; scope: BindingScope; label: string } & BindingShape;

export const SUMMARY_SEPARATOR = " · ";

export function placeholder(key: string): string {
  return `{${key}}`;
}

export function summaryOf(keys: string[], separator = SUMMARY_SEPARATOR): BindingSummary {
  return { parts: keys.map((key, i) => (i === 0 ? placeholder(key) : `${separator}${placeholder(key)}`)), separator };
}

function defaultSummary(shape: BindingShape): BindingSummary {
  if (shape.each) return summaryOf([shape.each.key]);
  if (shape.fields && shape.fields.length > 0) return summaryOf(shape.fields.map((field) => field.key));
  return summaryOf(["label"]);
}

export function binding(capability: string, scope: BindingScope, label: string, shape: BindingShape = {}): BindingSpec {
  return { capability, scope, label, ...shape, summary: shape.summary ?? defaultSummary(shape) };
}

export function bindingField(key: string, label: string, required = false, skippable = false): BindingField {
  return skippable ? { key, label, required, skippable } : { key, label, required };
}

export function options(capability: string): Action {
  const option = object([["value", "string"], ["label", "string"]], [["detail", "string"]]);
  const action = read(`${capability}.options`, object([], [["query", "string"], ["field", "string"], ["chosen", "object"], ["names", "array"], ["page", "integer"]]), listOf(option), looks());
  return { ...action, description: `List the choices for a ${capability} field, optionally matching a query.` };
}

export type Option = { value: string; label: string };

export function namesOf(args: Record<string, unknown> | undefined): string[] {
  const given = Array.isArray(args?.names) ? args.names : [];
  const names = given.filter((name): name is string => typeof name === "string").map((name) => name.trim()).filter(Boolean);
  return [...new Set(names)];
}

export function plainName(text: string): string {
  return text.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");
}

export function bestMatch<T extends Option>(options: T[], names: string[], piecesOf: (option: T) => string[] = (option) => [option.value, option.label]): T[] {
  for (const name of names.map(plainName).filter(Boolean)) {
    const found = options.find((option) => piecesOf(option).some((piece) => plainName(piece) === name));
    if (found) return [found];
  }
  return [];
}

export function suggestBy<T extends Option>(names: string[], search: (query: string) => T[], piecesOf?: (option: T) => string[]): T[] {
  for (const query of names) {
    const found = bestMatch(search(query), names, piecesOf);
    if (found.length > 0) return found;
  }
  return [];
}

export function projectOptions(found: unknown): { value: string; label: string }[] {
  const listed = Array.isArray(found) ? found : [];
  return listed
    .map((project) => ({ key: String(project?.key ?? ""), name: String(project?.name ?? "") }))
    .filter((project) => project.key !== "")
    .map((project) => ({ value: project.key, label: project.name || project.key }));
}
