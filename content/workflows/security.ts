import { workflow, on } from "sdk";
import { findings, ticket, develop, checks, pr, review } from "sdk/steps";

export default workflow({
  name: "security",
  triggers: [on("schedule", { cron: "0 8 * * *", applications: "security", title: "Vulnerabilidades" })],
  steps: [
    findings({ id: "plan", minSeverity: 3, batch: 5, status: { category: "todo" } }),
    ticket(),
    develop({ agent: "security-fixer", status: { category: "doing" } }),
    checks({ agent: "verifier", onFail: "develop", retries: 3 }),
    pr(),
    review({ status: { category: "doing" }, onChanges: "develop", doneStatus: { category: "done" } }),
  ],
});
