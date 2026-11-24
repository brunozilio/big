import { workflow, on } from "sdk";
import { develop, checks, ci, review, merge, release } from "sdk/steps";

export default workflow({
  name: "pull-request",
  accepts: ["pull_request"],
  triggers: [on("code.review.submitted", { verdict: "request_changes" })],
  statuses: ["A fazer", "Fazendo", "Revisando", "Feito"],
  steps: [
    develop({ agent: "developer", status: "Fazendo", feedbackOnly: true }),
    checks({ onFail: "develop", retries: 3 }),
    ci({ onFail: "develop", retries: 3 }),
    review({ agent: "reviewer", status: "Revisando", onChanges: "develop" }),
    merge({ method: "squash" }),
    release({ agent: "release", cloud: true, doneStatus: "Feito" }),
  ],
});
