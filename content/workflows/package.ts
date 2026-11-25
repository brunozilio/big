import { workflow, on } from "sdk";
import { arrivals, release } from "sdk/release/index.js";
import config from "../packages/config.ts";

export default workflow({
  name: "package",
  triggers: arrivals(config).map((filter) => on("item.step.finished", filter, { route: "accepting", title: config.labels.title })),
  steps: [release(config)],
});
