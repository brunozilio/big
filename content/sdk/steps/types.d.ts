import type { Json, MergeMethod, RetryPolicy, ReviewLine, StepType } from "../index";

export interface Sources {
  plan?: string;
  ticket?: string;
  develop?: string;
  pr?: string;
  merge?: string;
}

export interface EffectStepOptions {
  connection?: string;
  timeout?: number;
  retry?: RetryPolicy;
  sources?: Sources;
}

export interface AgentStepOptions extends EffectStepOptions {
  agent?: string;
  model?: string;
  mode?: string;
  prompt?: string;
  worktree?: string;
  askTimeout?: number;
}

export interface HumanStepOptions {
  waitTimeout?: number;
}

export interface PipelineStepOptions extends EffectStepOptions {
  pipeline?: string;
  ref?: string;
  every?: number;
}

export interface PlanOptions extends AgentStepOptions {}

export interface PlanOutput {
  session: string;
  plan: string;
  title: string;
}

export interface TicketOptions extends EffectStepOptions {
  issue?: string;
  project?: string;
  title?: string;
  plan?: string;
  description?: string;
  type?: string;
  labels?: string[];
  parent?: string;
}

export interface TicketOutput {
  issue: string;
  url?: string;
  title?: string;
  created: boolean;
}

export interface DevelopOptions extends AgentStepOptions {
  plan?: string;
  title?: string;
  issue?: string;
  branch?: string;
  feedback?: { step: string; outcome: string; data: Json } | null;
}

export interface DevelopOutput {
  session: string;
  branch?: string;
  summary?: string;
}

export interface ChecksOptions {
  commands?: string[];
  cwd?: string;
  env?: Record<string, string>;
  timeout?: number;
  retry?: RetryPolicy;
}

export interface ChecksOutput {
  output: string;
  command?: string;
  code?: number | null;
}

export interface PrOptions extends EffectStepOptions {
  title?: string;
  plan?: string;
  body?: string;
  issue?: string;
  branch?: string;
  base?: string;
  draft?: boolean;
}

export interface PrOutput {
  pr: string;
  number: number | null;
  url: string | null;
  title: string;
  body: string;
  head: string;
  base: string | null;
}

export interface CiOptions extends PipelineStepOptions {
  pr?: string;
  branch?: string;
}

export interface RunOutput {
  run: string;
  state: string;
  url: string | null;
  seen: string[];
  summary?: string;
  output?: string;
}

export interface ReviewOptions extends AgentStepOptions, HumanStepOptions {
  pr?: string;
  plan?: string;
  onChanges?: string | null | false;
  actions?: string[];
}

export interface ReviewOutput {
  pr: string;
  verdict: "approve" | "request_changes";
  body?: string | null;
  comments?: { path?: string; line?: number; body: string; author?: string }[];
  lines?: ReviewLine[];
  by?: string | null;
  session?: string;
}

export interface MergeOptions extends EffectStepOptions, HumanStepOptions {
  pr?: string;
  method?: MergeMethod;
  methods?: MergeMethod[];
  manual?: boolean;
}

export interface MergeOutput {
  pr: string;
  method: MergeMethod;
  methods: MergeMethod[];
  sha: string | null;
}

export interface ReleaseOptions extends PipelineStepOptions, HumanStepOptions {
  pr?: string;
  approve?: "human" | "auto";
}

export type PlanStep = StepType<PlanOptions>;
export type TicketStep = StepType<TicketOptions>;
export type DevelopStep = StepType<DevelopOptions>;
export type ChecksStep = StepType<ChecksOptions>;
export type PrStep = StepType<PrOptions>;
export type CiStep = StepType<CiOptions>;
export type ReviewStep = StepType<ReviewOptions>;
export type MergeStep = StepType<MergeOptions>;
export type ReleaseStep = StepType<ReleaseOptions>;
