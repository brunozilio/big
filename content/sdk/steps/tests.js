import { step, markdown } from "sdk";
import { agentOptions, branchOf, defined, object, planOf } from "./context.js";

function request(plan) {
  const parts = [`Escreva os cenários de teste do item: ${plan.title}`];
  if (plan.story) parts.push(plan.story);
  else if (plan.text) parts.push(plan.text);
  return parts.join("\n\n");
}

function cleaned(text) {
  const found = String(text ?? "").match(/```gherkin\s*\n([\s\S]*?)```/);
  const body = found ? found[1] : String(text ?? "");
  return body.trim();
}

export default step({
  name: "tests",
  label: "Testes",
  icon: "sTest",
  doing: "Planejando testes",
  view: (s) => (s.scenarios ? [markdown("```gherkin\n" + s.scenarios + "\n```")] : []),
  async run(ctx, opts) {
    const previous = object(ctx.output(ctx.step));
    if (previous.scenarios && !ctx.from) return ctx.pass(previous);
    const plan = planOf(ctx, opts);
    const input = defined({ prompt: opts.prompt ?? request(plan), title: plan.title });
    const worktree = opts.worktree ?? branchOf(ctx, opts) ?? ctx.item.id;
    const result = await ctx.agent(opts.agent ?? "tester", input, agentOptions(opts, { worktree }));
    const scenarios = cleaned(result.plan ?? result.text);
    if (!scenarios) return ctx.fail({ session: result.session, error: "o agente não escreveu nenhum cenário" });
    return ctx.pass({ session: result.session, scenarios });
  },
});
