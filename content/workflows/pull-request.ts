import { workflow, on } from "sdk";
import { develop, checks, ci, review, merge, release } from "sdk/steps";

export default workflow({
  name: "pull-request",
  accepts: ["pull_request"],
  triggers: [on("code.review.submitted", { verdict: "request_changes" })],
  statuses: ["A fazer", "Fazendo", "Feito"],
  steps: [
    develop({ agent: "developer", status: "Fazendo", feedbackOnly: true }),
    checks({ onFail: "develop", retries: 3 }),
    ci({ onFail: "develop", retries: 3 }),
    review({ agent: "reviewer", status: "Fazendo", onChanges: "develop" }),
    merge({ method: "squash" }),
    release({ agent: "release", cloud: true, doneStatus: "Feito" }),
  ],
});
