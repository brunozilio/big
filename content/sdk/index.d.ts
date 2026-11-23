import type { ActionArgs, ActionName, ActionResult, CapabilityName } from "./capabilities";

export type * from "./capabilities";

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

export interface RetryPolicy {
  max?: number;
  backoff?: "exponential" | "fixed";
  initial?: number;
  maxInterval?: number;
  jitter?: number | boolean;
}

export interface EffectOptions {
  retry?: RetryPolicy;
  timeout?: number;
}

export interface CallOptions extends EffectOptions {
  connection?: string;
  optional?: boolean;
}

export type ErrorKind =
  | "transient"
  | "rate_limited"
  | "auth"
  | "conflict"
  | "permanent"
  | "timeout"
  | "cancelled"
  | "error";

export declare class EffectError extends Error {
  readonly kind: ErrorKind;
  readonly data: Json;
  readonly seq: number;
}

export declare class StepFailed extends Error {
  readonly step: string;
  readonly outcome: Outcome;
}

export declare class Cancelled extends Error {
  readonly reason: string | null;
}

export type Button = string | { id: string; label: string; style?: "primary" | "danger" | "default" };

export interface FormField {
  id: string;
  label: string;
  kind: "text" | "textarea" | "select" | "checkbox" | "number";
  options?: string[];
  required?: boolean;
  value?: Json;
}

export type Block =
  | { type: "chat"; session: string; composer: boolean }
  | { type: "issue"; ref: string }
  | { type: "change"; pr: string }
  | { type: "review"; pr: string; session?: string }
  | { type: "pipeline"; run: string }
  | { type: "merge"; pr: string; methods: string[] }
  | { type: "markdown"; text: string }
  | { type: "cases"; cases: CaseBlockEntry[]; session: string | null; approve: string | null; changes: string | null }
  | { type: "logs"; text: string }
  | { type: "logs"; commands: LogCommand[] }
  | { type: "form"; fields: FormField[] }
  | { type: "actions"; buttons: { id: string; label: string; style?: string }[] };

export type View = Block | Block[];

export declare function chat(session: string, options?: { composer?: boolean }): Block;
export declare function issue(ref: string): Block;
export declare function change(pr: string): Block;
export declare function review(pr: string, options?: { session?: string }): Block;
export declare function pipeline(run: string): Block;
export declare function merge(pr: string, methods: string[]): Block;
export declare function markdown(text: string): Block;
export interface CaseBlockEntry {
  title: string;
  kind: "api" | "ui";
  objective?: string;
  precondition?: string;
  steps: { action: string; data?: string; expected: string }[];
  ref?: string;
  url?: string | null;
}
export declare function testCases(cases: CaseBlockEntry[], options?: { session?: string; approve?: string; changes?: string }): Block;
export interface LogCommand {
  name: string;
  state: "done" | "failed" | "running" | "todo";
  seconds: number | null;
  log: string;
}
export declare function logs(input: string | LogCommand[]): Block;
export declare function form(fields: FormField[]): Block;
export declare function actions(buttons: Button[]): Block;

export interface Trigger {
  type: "trigger";
  kind: string;
  filter: Record<string, Json>;
}

export declare function on(kind: string, filter?: Record<string, Json>): Trigger;

export type Outcome =
  | { type: "outcome"; outcome: "passed"; data: Json }
  | { type: "outcome"; outcome: "failed"; data: Json }
  | { type: "outcome"; outcome: "skipped"; data: Json }
  | { type: "outcome"; outcome: "goto"; to: string; data: Json }
  | { type: "outcome"; outcome: "cancelled"; reason: string | null };

export interface AgentInput {
  prompt: string;
  session?: string;
  [key: string]: Json | undefined;
}

export interface AgentOptions extends CallOptions {
  model?: string;
  effort?: "low" | "medium" | "high" | "xhigh" | "max";
  mode?: string;
  worktree?: string;
}

export interface AgentRepository {
  id: string;
  name: string;
  worktree: string;
  base: string | null;
  commit: string | null;
  changed: boolean;
}

export interface AgentResult {
  text: string;
  session: string;
  output: Json;
  plan?: string | null;
  repositories?: AgentRepository[];
}

export interface ExecOptions extends EffectOptions {
  repository?: string;
  cwd?: string;
  env?: Record<string, string>;
  input?: string;
}

export interface ExecResult {
  code: number | null;
  output: string;
  stdout: string;
  stderr: string;
}

export interface HumanAnswer {
  action: string;
  values: Record<string, Json>;
  by: string | null;
}

export interface PollTarget<C extends string = string, A extends string = string> {
  capability: C;
  action: A;
  args?: Json;
  connection?: string;
}

export interface PollOptions extends EffectOptions {
  until?: Json;
}

export interface ExternalEvent {
  id: string;
  connection: string;
  capability: string;
  kind: string;
  subject: string;
  data: Json;
  at: string;
}

export interface Fork<T = unknown> {
  index: number;
  branch: string;
  kind: string;
  value: T;
}

export type Branch<T> = Promise<T> | ((ctx: Ctx) => Promise<T> | T);

type Settled<T> = T extends Branch<infer R> ? R : never;

export interface StepEntry {
  step: string;
  outcome: "failed" | "goto";
  data: Json;
}

export interface Ctx {
  readonly branch: string;
  readonly step: string | null;
  readonly attempt: number;
  readonly item: WorkItem;
  readonly from: StepEntry | null;
  output<T = Json>(step: string): T | null;
  agent(name: string, input: string | AgentInput, options?: AgentOptions): Promise<AgentResult>;
  exec(command: string, options?: ExecOptions): Promise<ExecResult>;
  call<C extends CapabilityName, A extends ActionName<C>>(
    capability: C,
    action: A,
    args: ActionArgs<C, A>,
    options?: CallOptions,
  ): Promise<ActionResult<C, A>>;
  call(capability: string, action: string, args?: Json, options?: CallOptions): Promise<Json>;
  waitHuman(view: View, options?: EffectOptions): Promise<HumanAnswer>;
  poll<C extends CapabilityName, A extends ActionName<C>>(
    call: PollTarget<C, A> & { args: ActionArgs<C, A> },
    every: number,
    options?: PollOptions,
  ): Promise<ActionResult<C, A>>;
  poll(call: PollTarget, every: number, options?: PollOptions): Promise<Json>;
  event(kind: string, filter?: Record<string, Json>, options?: EffectOptions): Promise<ExternalEvent>;
  sleep(ms: number): Promise<null>;
  all<const T extends readonly Branch<unknown>[]>(branches: T): Promise<{ [K in keyof T]: Settled<T[K]> }>;
  race<const T extends readonly Branch<unknown>[]>(branches: T): Promise<Fork<Settled<T[number]>>>;
  any<const T extends readonly Branch<unknown>[]>(branches: T): Promise<Fork<Settled<T[number]>>>;
  child(workflow: string, input?: Json, options?: EffectOptions): Promise<Json>;
  status(name: string, options?: CallOptions): Promise<null>;
  impediment(on: boolean, reason?: string, options?: CallOptions): Promise<null>;
  pass(data?: Json): Outcome;
  fail(data?: Json): Outcome;
  skip(data?: Json): Outcome;
  goto(step: string, data?: Json): Outcome;
  cancel(reason: string): Outcome;
  now(): number;
  random(): number;
}

export interface StepOptions {
  id?: string;
  status?: string;
  doneStatus?: string;
  onFail?: string;
  retries?: number;
  [key: string]: unknown;
}

export interface StepCall<O = StepOptions> {
  type: "step-call";
  step: string;
  id: string;
  options: O & StepOptions;
}

export type StepViewState<D = Record<string, Json>> = D & {
  id: string;
  state: "pending" | "running" | "waiting_human" | "passed" | "failed" | "skipped";
  attempt: number;
  passed: boolean;
  failed: boolean;
};

export interface StepDefinition<O, D> {
  name: string;
  label?: string;
  icon?: string;
  needs?: string;
  doing?: string;
  view?(state: StepViewState<D>): View;
  run(ctx: Ctx, options: O & StepOptions): Promise<Outcome | Json | void> | Outcome | Json | void;
}

export interface StepType<O> {
  (options?: O & StepOptions): StepCall<O>;
  (ctx: Ctx, options?: O & StepOptions): Promise<Outcome>;
  readonly stepName: string;
}

export declare function step<O = Record<string, unknown>, D = Record<string, Json>>(
  definition: StepDefinition<O, D>,
): StepType<O>;

export interface WorkItem {
  id: string;
  workflow: string;
  title: string;
  labels: string[];
  branch: string | null;
  issue: string | null;
  input: Json;
  [key: string]: Json | undefined;
}

export type ReconcilePolicy = "local" | "remote" | "event";

export interface Reconcile {
  status?: ReconcilePolicy;
  impediment?: ReconcilePolicy;
  pull_request?: ReconcilePolicy;
  pipeline?: ReconcilePolicy;
  [field: string]: ReconcilePolicy | undefined;
}

export type SourceKind = "issue" | "pull_request" | "branch" | "text";

export interface WorkflowDefinition {
  name: string;
  statuses?: string[];
  triggers?: Trigger[];
  reconcile?: Reconcile;
  accepts?: SourceKind[];
  steps?: (StepCall<unknown> | StepType<unknown>)[];
  run?(ctx: Ctx, item: WorkItem): Promise<Outcome | Json | void>;
}

export interface Workflow {
  type: "workflow";
  name: string;
  statuses: string[];
  triggers: Trigger[];
  reconcile: Reconcile;
  accepts: SourceKind[];
  steps: StepCall<unknown>[];
  code: boolean;
}

export declare function workflow(definition: WorkflowDefinition): Workflow;

export interface Command {
  seq: number;
  kind: "agent" | "exec" | "call" | "wait.human" | "wait.poll" | "event" | "sleep" | "child" | "status.set" | "impediment.set";
  branch: string;
  step: string | null;
  attempt: number;
  args: Json;
  connection?: string;
  retry?: RetryPolicy;
  timeout?: number;
  optional?: boolean;
}

export interface Signal {
  seq: number;
  kind: "step.started" | "step.finished" | "fork" | "branch.settled";
  branch: string;
  step: string | null;
  attempt: number;
  [key: string]: Json | undefined;
}

export interface JournalEntry {
  seq: number;
  kind?: Command["kind"];
  ok?: Json;
  error?: { kind: ErrorKind; message?: string; data?: Json };
  cancelled?: boolean;
  at?: number;
}

export interface Activation {
  target?: { workflow: string } | { step: string; id?: string; options?: Json };
  item?: WorkItem;
  journal?: JournalEntry[];
  start?: number;
  seed?: string;
}

export interface ActivationResult {
  state: "running" | "completed" | "failed" | "cancelled" | "nondeterministic";
  result?: Json;
  reason?: string | null;
  error?: { kind: string; message: string; data?: Json };
  pending: Command[];
  cancel: number[];
  signals: Signal[];
  seq: number;
}

export declare const PROTOCOL: 1;

export declare const host: {
  readonly protocol: 1;
  activate(activation: Activation): Promise<ActivationResult>;
  view(request: { step: string; state: StepViewState }): Block[];
  describe(): { workflows: Workflow[]; steps: string[] };
};
