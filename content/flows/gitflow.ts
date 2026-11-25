import { flow } from "sdk/flow.js";

export const spec = {
  match: { repo: [] },
  branch: "feature/{issue}-{slug}",
  base: "develop",
  method: "squash",
  byType: {
    Bug: { branch: "bugfix/{issue}-{slug}" },
    Hotfix: { branch: "hotfix/{issue}-{slug}", base: "main", method: "merge" },
  },
  promote: [
    { from: "develop", into: "release", method: "merge" },
    { from: "release", into: "main", method: "merge" },
  ],
};

export const branch = flow(spec);
