import { workflow } from "sdk";
import { aside } from "sdk/steps";

export default workflow({
  name: "pergunta",
  accepts: [],
  triggers: [],
  statuses: ["Conversando", "Feito"],
  steps: [aside({ agent: "aside", status: "Conversando", doneStatus: "Feito" })],
});
