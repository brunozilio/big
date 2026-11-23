export type PullRequestRef = string;

export type CheckState = "pending" | "running" | "passed" | "failed" | "skipped" | "cancelled";

export interface Check {
  name: string;
  state: CheckState;
  url: string | null;
}

export interface PullRequest {
  ref: PullRequestRef;
  number: number;
  title: string;
  body: string;
  url: string;
  head: string;
  base: string;
  state: "open" | "closed" | "merged";
  draft: boolean;
  mergeable: boolean | null;
  checks: Check[];
  updated: string;
}

export interface FileDiff {
  path: string;
  previous: string | null;
  change: "added" | "modified" | "removed" | "renamed";
  additions: number;
  deletions: number;
  patch: string;
}

export interface ReviewComment {
  id: string;
  thread: string;
  path: string | null;
  line: number | null;
  author: string;
  body: string;
  resolved: boolean;
  at: string;
}

export interface ReviewLine {
  path: string;
  line: number;
  body: string;
}

export type ReviewVerdict = "approve" | "comment" | "request_changes";

export type MergeMethod = "merge" | "squash" | "rebase";

export interface Code {
  open_pull_request: {
    args: { head: string; base?: string; title: string; body?: string; draft?: boolean };
    result: PullRequest;
  };
  pull_request: { args: { pr: PullRequestRef }; result: PullRequest };
  files_diff: { args: { pr: PullRequestRef }; result: FileDiff[] };
  review_comments: { args: { pr: PullRequestRef }; result: ReviewComment[] };
  submit_review: {
    args: { pr: PullRequestRef; verdict: ReviewVerdict; body?: string; lines?: ReviewLine[] };
    result: { id: string; verdict: ReviewVerdict };
  };
  resolve_thread: { args: { pr: PullRequestRef; thread: string }; result: { thread: string; resolved: boolean } };
  merge: { args: { pr: PullRequestRef; method: MergeMethod }; result: { merged: boolean; sha: string | null } };
}
