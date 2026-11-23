import { step, markdown } from "sdk";
import { object } from "sdk/steps/context.js";

export default step({
  name: "handoff",
  label: "Ajustar o portão",
  icon: "sCode",
  doing: "Pedindo ao laço externo para ajustar o portão",
  view: (s) => [markdown(s.child ? `O laço externo recebeu o caso: \`${s.child}\`` : "")],
  async run(ctx, opts) {
    const factory = object(ctx.item.factory);
    const data = object(factory.data);
    const input = {
      title: `Fábrica: fazer ${data.gate} pegar o defeito que escapou (${data.workflow})`,
      factory: { ...factory, kind: "factory.step.recurring", subject: `${factory.subject}/handoff`, data: { ...data, step: data.gate, count: 1 } },
    };
    const result = await ctx.child(opts.workflow ?? "factory-tune", input);
    return ctx.pass({ child: opts.workflow ?? "factory-tune", result });
  },
});
