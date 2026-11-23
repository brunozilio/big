export type IssueRef = string;

export interface Person {
  id: string;
  name: string;
}

export interface Attachment {
  id: string;
  name: string;
  url: string;
  mime?: string;
}

export interface IssueComment {
  id: string;
  author: Person | null;
  body: string;
  at: string;
}

export interface IssueSummary {
  ref: IssueRef;
  title: string;
  status: string;
  url: string;
}

export interface Issue extends IssueSummary {
  description: string;
  type: string | null;
  labels: string[];
  assignee: Person | null;
  impediment: { on: boolean; reason: string | null };
  subtasks: IssueSummary[];
  attachments: Attachment[];
  comments: IssueComment[];
  updated: string;
}

export interface RemoteStatus {
  id: string;
  name: string;
  category: "todo" | "doing" | "done" | null;
}

export interface Tracker {
  create_issue: {
    args: { project?: string; title: string; description?: string; type?: string; labels?: string[]; parent?: IssueRef };
    result: IssueSummary;
  };
  issue: { args: { issue: IssueRef }; result: Issue };
  comment: { args: { issue: IssueRef; body: string }; result: IssueComment };
  statuses: { args: { project?: string }; result: RemoteStatus[] };
  transition: { args: { issue: IssueRef; status: string }; result: IssueSummary };
  set_impediment: { args: { issue: IssueRef; on: boolean; reason?: string }; result: Issue["impediment"] };
  search: { args: { query: string; limit?: number; cursor?: string }; result: { issues: IssueSummary[]; cursor: string | null } };
}
