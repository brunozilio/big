import { workflow, on } from "sdk";
import { arrivals, release } from "sdk/release/index.js";
import config from "../packages/dev/release.ts";

export default workflow({
  schema_version: 1,
  name: "package",
  triggers: arrivals(config).map((filter) => on("item.step.finished", filter, { route: "accepting", title: config.labels.title })),
  steps: [release(config)],
});
