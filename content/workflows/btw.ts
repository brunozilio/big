import { workflow } from "sdk";
import { aside } from "sdk/steps";

export default workflow({
  schema_version: 1,
  name: "pergunta",
  accepts: [],
  triggers: [],
  statuses: ["Conversando", "Feito"],
  steps: [aside({ agent: "aside", status: "Conversando", doneStatus: "Feito" })],
});
