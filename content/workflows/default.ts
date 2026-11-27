import { workflow, on } from "sdk";
import { plan, ticket, cases, develop, checks, pr, ci, quality, security, review, provision, merge, release } from "sdk/steps";
import { environments } from "../environments.ts";
import { branches } from "../branches.ts";

export default workflow({
  name: "entrega",
  accepts: ["issue", "text"],
  triggers: [on("tracker.issue.created")],
  branches,
  steps: [
    plan({ agent: "planner", status: { category: "todo" } }),
    ticket(),
    cases({ agent: "tester" }),
    develop({ agent: "developer", status: { category: "doing" } }),
    checks({ agent: "verifier", tester: "tester", onFail: "develop", retries: 3 }),
    pr(),
    ci({ onFail: "develop", retries: 3 }),
    quality({ onFail: "develop", retries: 3 }),
    review({ agent: "reviewer", status: { category: "doing" }, onChanges: "develop" }),
    provision({ cloud: true, environments }),
    merge({ method: "squash" }),
    release({ agent: "release", cloud: true, environments, doneStatus: { category: "done" } }),
    security(),
  ],
});
