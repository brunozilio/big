import { workflow, on } from "sdk";
import { plan, ticket, cases, develop, checks, pr, ci, review, merge, release } from "sdk/steps";

export default workflow({
  name: "entrega",
  accepts: ["issue", "text"],
  triggers: [on("tracker.issue.created")],
  statuses: ["A fazer", "Fazendo", "Feito"],
  steps: [
    plan({ agent: "planner", status: "A fazer" }),
    ticket(),
    cases({ agent: "tester" }),
    develop({ agent: "developer", status: "Fazendo" }),
    checks({ agent: "verifier", tester: "tester", onFail: "develop", retries: 3 }),
    pr(),
    ci({ onFail: "develop", retries: 3 }),
    review({ agent: "reviewer", status: "Fazendo", onChanges: "develop" }),
    merge({ method: "squash" }),
    release({ agent: "release", cloud: true, doneStatus: "Feito" }),
  ],
});
