import type { Ctx, Json, Run, RunSummary, StepEntry } from "../index";
import type { AgentStepOptions, EffectStepOptions, HumanStepOptions, PipelineStepOptions, PrOutput } from "./types";

export declare const FINISHED: string[];
export declare function defined<T extends Record<string, unknown>>(fields: T): Partial<T>;
export declare function effects(opts: EffectStepOptions): { connection?: string; timeout?: number; retry?: Json };
export declare function agentOptions(opts: AgentStepOptions, extra?: Record<string, unknown>): Record<string, Json>;
export declare function tail(text: unknown, limit?: number): string;
export declare function object(value: unknown): Record<string, any>;
export declare function source(ctx: Ctx, opts: EffectStepOptions, kind: string): Record<string, any>;
export declare function planOf(ctx: Ctx, opts: { title?: string; plan?: string } & EffectStepOptions): { title: string; text: string };
export declare function issueOf(ctx: Ctx, opts: { issue?: string } & EffectStepOptions): string | null;
export declare function branchOf(ctx: Ctx, opts: { branch?: string } & EffectStepOptions): string | null;
export declare function pullRequestOf(ctx: Ctx, opts: { pr?: string } & EffectStepOptions): Partial<PrOutput> & { pr: string } | null;
export declare function waitOptions(opts: HumanStepOptions): { timeout?: number };
export declare function feedbackText(from: StepEntry | null): string;
export declare function findRun(ctx: Ctx, opts: PipelineStepOptions, wanted: { ref: string; seen: string[] }): Promise<RunSummary>;
export declare function settle(ctx: Ctx, opts: PipelineStepOptions, run: string, until: string[], events: string[]): Promise<Run>;
export declare function failedLogs(ctx: Ctx, opts: PipelineStepOptions, run: Run): Promise<string>;
