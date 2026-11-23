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
  size?: number;
}

export interface IssueComment {
  id: string;
  author: Person | null;
  body: string;
  at: string;
  created?: boolean;
}

export interface IssueSummary {
  ref: IssueRef;
  title: string;
  status: string;
  url: string;
  created?: boolean;
}

export interface Subtask extends IssueSummary {
  done: boolean;
}

export interface Issue extends IssueSummary {
  description: string;
  type: string | null;
  labels: string[];
  assignee: Person | null;
  impediment: { on: boolean; reason: string | null };
  subtasks: Subtask[];
  attachments: Attachment[];
  comments: IssueComment[];
  updated: string;
}

export interface Project {
  id: string;
  key: string;
  name: string;
  avatar?: string | null;
}

export interface RemoteStatus {
  id: string;
  name: string;
  category: string | null;
}

export interface Tracker {
  create_issue: {
    args: { project?: string; title: string; description?: string; type?: string; labels?: string[]; parent?: IssueRef };
    result: IssueSummary;
  };
  issue: { args: { issue: IssueRef }; result: Issue };
  comment: { args: { issue: IssueRef; body: string }; result: IssueComment };
  statuses: { args: { project?: string }; result: RemoteStatus[] };
  transition: { args: { issue: IssueRef; status: string }; result: { ref: IssueRef; status: string; changed: boolean } };
  set_impediment: { args: { issue: IssueRef; on: boolean; reason?: string }; result: { on: boolean; reason: string | null } };
  search: {
    args: { query?: string; project?: string; status?: string; limit?: number; cursor?: string };
    result: { issues: IssueSummary[]; cursor: string | null };
  };
  projects: { args: { query?: string }; result: Project[] };
}
