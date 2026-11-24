import { step, markdown } from "sdk";
import { object } from "sdk/steps/context.js";

export default step({
  name: "handoff",
  label: "Adjust the gate",
  icon: "sCode",
  doing: "Asking the outer loop to adjust the gate",
  view: (s) => [markdown(s.child ? `The outer loop received the case: \`${s.child}\`` : "")],
  async run(ctx, opts) {
    const factory = object(ctx.item.factory);
    const data = object(factory.data);
    const input = {
      title: `Factory: make ${data.gate} catch the defect that escaped (${data.workflow})`,
      factory: { ...factory, kind: "factory.step.recurring", subject: `${factory.subject}/handoff`, data: { ...data, step: data.gate, count: 1 } },
    };
    const result = await ctx.child(opts.workflow ?? "factory-tune", input);
    return ctx.pass({ child: opts.workflow ?? "factory-tune", result });
  },
});
