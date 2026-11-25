import { step, chat, markdown } from "sdk";
import { agentOptions, branchOf, defined, inLanguage } from "./context.js";

function rejected(opts, text, result) {
  if (typeof opts.check === "function") return !opts.check(result);
  if (typeof opts.reject === "string" && opts.reject) return new RegExp(opts.reject, "i").test(text);
  return false;
}

export default step({
  name: "agent",
  label: "Agent",
  icon: "sCode",
  doing: "Working",
  view: (s) => (s.session ? [chat(s.session)] : s.text ? [markdown(s.text)] : []),
  async run(ctx, opts) {
    if (typeof opts.agent !== "string" || !opts.agent) return ctx.fail({ error: "agent({ agent }) needs the name of an agent" });
    const prompt = typeof opts.prompt === "function" ? await opts.prompt(ctx) : String(opts.prompt ?? ctx.item.title ?? "");
    const worktree = opts.worktree ?? branchOf(ctx, opts) ?? ctx.item.id;
    const result = await ctx.agent(opts.agent, defined({ prompt: inLanguage(ctx, prompt) }), agentOptions(opts, { worktree }));
    const text = String(result.text ?? "");
    const data = defined({ session: result.session, text, output: result.output });
    if (rejected(opts, text, result)) return ctx.fail({ ...data, error: `${opts.agent} rejected the work` });
    return ctx.pass(data);
  },
});
