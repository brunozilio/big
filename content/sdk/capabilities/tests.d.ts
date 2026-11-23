import type { IssueRef } from "./tracker";

export type TestCaseRef = string;

export type TestCycleRef = string;

export type TestStatus = "passed" | "failed" | "blocked" | "not_run";

export interface TestStep {
  action: string;
  data?: string;
  expected: string;
}

export interface TestCase {
  ref: TestCaseRef;
  title: string;
}

export interface TestProject {
  id: string;
  key: string;
  name: string;
}

export interface Tests {
  create_case: {
    args: {
      title: string;
      project?: string;
      objective?: string;
      precondition?: string;
      labels?: string[];
      steps?: TestStep[];
      issue?: IssueRef;
      issue_id?: string | number;
    };
    result: TestCase & { url: string; created?: boolean };
  };
  cases: { args: { issue: IssueRef }; result: TestCase[] };
  create_cycle: {
    args: { title: string; project?: string; description?: string; issue?: string; issue_id?: string | number };
    result: { ref: TestCycleRef; url: string };
  };
  record: {
    args: {
      case: TestCaseRef;
      cycle: TestCycleRef;
      status: TestStatus;
      project?: string;
      comment?: string;
      seconds?: number;
    };
    result: { ref: string };
  };
  projects: { args: { query?: string }; result: TestProject[] };
}
