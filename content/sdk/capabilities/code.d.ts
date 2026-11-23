import type { RunState } from "./pipelines";

export type PullRequestRef = string;

export type CheckState = RunState;

export interface Check {
  name: string;
  state: CheckState;
  url: string | null;
}

export type ReviewVerdict = "approve" | "comment" | "request_changes";

export interface Review {
  id: string;
  author: string;
  verdict: ReviewVerdict | "dismissed" | "pending";
  body: string;
  at: string;
}

export interface PullRequest {
  ref: PullRequestRef;
  repo: string;
  number: number;
  title: string;
  body: string;
  url: string;
  head: string;
  base: string;
  sha: string;
  state: "open" | "closed" | "merged";
  merged: boolean;
  draft: boolean;
  mergeable: boolean | null;
  author: string;
  checks: Check[];
  reviews: Review[];
  updated: string;
  created?: boolean;
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
  outdated: boolean;
  at: string;
}

export interface ReviewLine {
  path: string;
  line: number;
  body: string;
}

export type MergeMethod = "merge" | "squash" | "rebase";

export interface Code {
  open_pull_request: {
    args: { head: string; base?: string; repo?: string; title: string; body?: string; draft?: boolean };
    result: PullRequest;
  };
  pull_request: { args: { pr: PullRequestRef }; result: PullRequest };
  files_diff: { args: { pr: PullRequestRef }; result: FileDiff[] };
  review_comments: { args: { pr: PullRequestRef }; result: ReviewComment[] };
  submit_review: {
    args: { pr: PullRequestRef; verdict: ReviewVerdict; body?: string; lines?: ReviewLine[] };
    result: { id: string; verdict: ReviewVerdict; created?: boolean };
  };
  resolve_thread: { args: { pr?: PullRequestRef; thread: string }; result: { thread: string; resolved: boolean } };
  merge: {
    args: { pr: PullRequestRef; method?: MergeMethod };
    result: { merged: boolean; sha: string | null; already?: boolean };
  };
}
