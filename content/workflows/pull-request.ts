import { workflow, on } from "sdk";
import { develop, checks, ci, review, merge, release } from "sdk/steps";

export default workflow({
  name: "pull-request",
  accepts: ["pull_request"],
  triggers: [on("code.review.submitted", { verdict: "request_changes" })],
  steps: [
    develop({ agent: "developer", status: { category: "doing" }, feedbackOnly: true }),
    checks({ onFail: "develop", retries: 3 }),
    ci({ onFail: "develop", retries: 3 }),
    review({ agent: "reviewer", status: { category: "doing" }, onChanges: "develop" }),
    merge({ method: "squash" }),
    release({ agent: "release", cloud: true, doneStatus: { category: "done" } }),
  ],
});
