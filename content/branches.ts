import { flow } from "sdk/flow.js";

const gitflow = flow({
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
});

const trunk = flow({
  branch: "feature/{issue}-{slug}",
  method: "squash",
  byType: {
    Bug: { branch: "bugfix/{issue}-{slug}" },
    Hotfix: { branch: "hotfix/{issue}-{slug}" },
  },
});

export function branches(asked) {
  if (asked.repository?.kind === "configuration") return trunk(asked);
  return gitflow(asked);
}
