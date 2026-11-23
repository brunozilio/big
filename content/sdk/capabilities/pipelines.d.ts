export type RunRef = string;

export type RunState = "queued" | "running" | "passed" | "failed" | "cancelled" | "waiting";

export interface PipelineStep {
  name: string;
  state: RunState;
  started: string | null;
  finished: string | null;
}

export interface Job {
  id: string;
  name: string;
  state: RunState;
  needs: string[];
  steps: PipelineStep[];
  url: string | null;
}

export interface RunSummary {
  ref: RunRef;
  pipeline: string;
  head: string;
  state: RunState;
  url: string;
  started: string | null;
  finished: string | null;
}

export interface Run extends RunSummary {
  jobs: Job[];
}

export interface PendingDeployment {
  id: string;
  environment: string;
  run: RunRef;
  reviewers: string[];
  canApprove: boolean;
}

export interface Pipelines {
  runs: { args: { ref: string; pipeline?: string; limit?: number }; result: RunSummary[] };
  run: { args: { run: RunRef }; result: Run };
  job_log: { args: { run: RunRef; job: string }; result: { text: string } };
  pending_deployments: { args: { run: RunRef }; result: PendingDeployment[] };
  review_deployment: {
    args: { run: RunRef; deployments: string[]; decision: "approve" | "reject"; comment?: string };
    result: { decision: "approve" | "reject"; deployments: string[] };
  };
}
