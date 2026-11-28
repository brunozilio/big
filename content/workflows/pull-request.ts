import { workflow, on } from "sdk";
import { develop, checks, ci, review, merge, release } from "../packages/dev/steps/index.js";
import { environments } from "../environments.ts";
import { branches } from "../branches.ts";

export default workflow({
  schema_version: 1,
  name: "pull-request",
  accepts: ["pull_request"],
  triggers: [on("code.review.submitted", { verdict: "request_changes" })],
  branches,
  steps: [
    develop({ agent: "developer", status: { category: "doing" }, feedbackOnly: true }),
    checks({ onFail: "develop", retries: 3 }),
    ci({ onFail: "develop", retries: 3 }),
    review({ agent: "reviewer", status: { category: "doing" }, onChanges: "develop" }),
    merge({ method: "squash" }),
    release({ agent: "release", cloud: true, environments, doneStatus: { category: "done" } }),
  ],
});
