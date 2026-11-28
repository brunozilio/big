import { workflow, on } from "sdk";
import { develop } from "../packages/dev/steps/index.js";
import { brief, evals, promote, handoff } from "sdk/factory";

export default workflow({
  schema_version: 1,
  name: "factory-evals",
  triggers: [on("factory.gate.escaped")],
  steps: [
    brief({ id: "plan" }),
    develop({ agent: "eval-engineer" }),
    evals({ requireOrigin: true, onFail: "develop", retries: 3 }),
    promote(),
    handoff(),
  ],
});
