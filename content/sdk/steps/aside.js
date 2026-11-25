import { step, chat, actions } from "sdk";
import { agentOptions, defined, object, waitOptions } from "./context.js";

const CLOSE = "close";

export default step({
  name: "aside",
  label: "Aside",
  icon: "sPlan",
  doing: "Talking",
  view: (s) => (s.session ? [chat(s.session, { composer: true })] : []),
  async run(ctx, opts) {
    const item = ctx.item;
    const previous = object(ctx.output(ctx.step));
    const prompt = String(item.description ?? item.title ?? "").trim();
    const input = defined({ prompt, session: previous.session });
    const result = await ctx.agent(opts.agent ?? "aside", input, agentOptions(opts, { mode: opts.mode ?? "plan", inPlace: true }));
    const view = [chat(result.session, { composer: true }), actions([{ id: CLOSE, label: opts.closeLabel ?? "Encerrar" }])];
    for (;;) {
      const answer = await ctx.waitHuman(view, waitOptions(opts));
      if (answer.action === CLOSE) break;
    }
    return ctx.pass({ session: result.session });
  },
});
