import { workflow, on } from "sdk";
import { release } from "sdk/release/index.js";
import config from "../packages/config.ts";

export default workflow({
  name: "package",
  triggers: [on("item.step.finished", config.arrival.filter, { route: "accepting", title: config.labels.title })],
  steps: [release(config)],
});
