import { step, logs } from "sdk";
import { NOTHING, checked, collect, e2e, groupsOf, own, repoChecks, verifier } from "./verify.js";

export default step({
  name: "checks",
  label: "Checks",
  icon: "sTest",
  doing: "Running checks",
  view: (s) => {
    const commands = checked(s);
    return commands.length === 0 ? [logs(s.output ?? NOTHING)] : [logs(commands)];
  },
  steps: (opts) => {
    const shared = own(opts);
    return [
      repoChecks({ ...shared, id: "commands", forEach: (ctx) => groupsOf(ctx, shared), failFast: false }),
      e2e({ ...shared, id: "e2e" }),
      verifier({ ...shared, id: "verifier", if: () => Boolean(shared.agent) }),
    ];
  },
  collect,
});
