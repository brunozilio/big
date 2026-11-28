import { workflow } from "sdk";
import { findings, ticket, develop, checks, pr, review } from "../packages/dev/steps/index.js";
import { branches } from "../branches.ts";
import { pullRequest } from "../packages/dev/output.ts";

export default workflow({
  schema_version: 1,
  name: "security",
  output: pullRequest,
  branches,
  steps: [
    findings({ id: "plan", minSeverity: 3, batch: 5, status: { category: "todo" } }),
    ticket(),
    develop({ agent: "security-fixer", status: { category: "doing" } }),
    checks({ agent: "verifier", onFail: "develop", retries: 3 }),
    pr(),
    review({ status: { category: "doing" }, onChanges: "develop", doneStatus: { category: "done" } }),
  ],
});
