import { workflow } from "sdk";
import { plan, ticket, cases, develop, checks, pr, ci, review, merge, release } from "sdk/steps";

export default workflow({
  name: "entrega",
  accepts: ["issue", "text"],
  statuses: ["A fazer", "Fazendo", "Revisando", "Feito"],
  steps: [
    plan({ agent: "planner", status: "A fazer" }),
    ticket(),
    cases({ agent: "tester" }),
    develop({ agent: "developer", status: "Fazendo" }),
    checks({ agent: "verifier", tester: "tester", onFail: "develop", retries: 3 }),
    pr(),
    ci({ onFail: "develop", retries: 3 }),
    review({ agent: "reviewer", status: "Revisando", onChanges: "develop" }),
    merge({ method: "squash" }),
    release({ agent: "release", cloud: true, doneStatus: "Feito" }),
  ],
});
