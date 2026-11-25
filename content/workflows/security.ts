import { workflow, on } from "sdk";
import { findings, ticket, develop, checks, pr, review } from "sdk/steps";

export default workflow({
  name: "security",
  triggers: [on("schedule", { cron: "0 8 * * *", applications: "security", title: "Vulnerabilidades" })],
  statuses: ["A fazer", "Fazendo", "Feito"],
  steps: [
    findings({ id: "plan", minSeverity: 3, batch: 5, status: "A fazer" }),
    ticket(),
    develop({ agent: "security-fixer", status: "Fazendo" }),
    checks({ agent: "verifier", onFail: "develop", retries: 3 }),
    pr(),
    review({ status: "Fazendo", onChanges: "develop", doneStatus: "Feito" }),
  ],
});
