import { workflow, on } from "sdk";
import { develop } from "sdk/steps";
import { brief, evals, promote } from "sdk/factory";

export default workflow({
  schema_version: 1,
  name: "factory-tune",
  triggers: [on("factory.step.recurring")],
  steps: [
    brief({ id: "plan" }),
    develop({ agent: "factory-engineer" }),
    evals({ onFail: "develop", retries: 3 }),
    promote(),
  ],
});
