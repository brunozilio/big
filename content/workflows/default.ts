import { workflow } from "sdk";
import { plan, ticket, tests, develop, checks, pr, ci, review, merge, release } from "sdk/steps";

export default workflow({
  name: "entrega",
  statuses: ["A fazer", "Fazendo", "Revisando", "Feito"],
  steps: [
    plan({ agent: "planner", status: "A fazer" }),
    ticket(),
    tests({ agent: "tester" }),
    develop({ agent: "developer", status: "Fazendo" }),
    checks({ agent: "verifier", onFail: "develop", retries: 3 }),
    pr(),
    ci({ onFail: "develop", retries: 3 }),
    review({ agent: "reviewer", status: "Revisando", onChanges: "develop" }),
    merge({ method: "squash" }),
    release({ agent: "release", cloud: true, doneStatus: "Feito" }),
  ],
});
