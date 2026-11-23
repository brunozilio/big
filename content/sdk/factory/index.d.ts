import type { StepType } from "../index";

export interface FactoryStepOptions {
  id?: string;
  autonomy?: "in" | "near" | "out";
  onFail?: string;
  retries?: number;
}

export interface EvalsOptions extends FactoryStepOptions {
  dir?: string;
  requireOrigin?: boolean;
  timeout?: number;
}

export interface PromoteOptions extends FactoryStepOptions {
  inherit?: boolean;
}

export interface HandoffOptions extends FactoryStepOptions {
  workflow?: string;
}

export interface EvalCase {
  name?: string;
  origin?: string;
  commit?: string;
  command: string;
  expect?: "fail" | "pass";
  head?: "pass" | "skip";
}

export declare const brief: StepType<FactoryStepOptions>;
export declare const evals: StepType<EvalsOptions>;
export declare const promote: StepType<PromoteOptions>;
export declare const handoff: StepType<HandoffOptions>;
export declare function briefOf(item: unknown): { title: string; plan: string; story: string };
export declare function parseCases(listing: string): { cases: EvalCase[]; broken: { file: string; error: string }[] };
