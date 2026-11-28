import { step, chat, markdown } from "sdk";
import { agentOptions, defined, feedbackText, inLanguage, object } from "./context.js";
import { fieldsText, given } from "./given.js";

async function asked(ctx, opts) {
  const written = await given(ctx, opts.prompt);
  const item = ctx.item;
  const parts = [written ? String(written) : `Draft: ${item.title ?? ""}`];
  if (!written && item.description) parts.push(String(item.description));
  const fields = fieldsText(await given(ctx, opts.fields));
  if (fields) parts.push(`## Fields\n\n${fields}`);
  const feedback = feedbackText(ctx.from);
  if (feedback) parts.push(`## Feedback\n\n${feedback}`);
  return inLanguage(ctx, parts.join("\n\n"));
}

export default step({
  name: "draft",
  label: "Draft",
  icon: "sPlan",
  doing: "Drafting",
  view: (s) => (s.session ? [chat(s.session)] : s.draft ? [markdown(s.draft)] : []),
  async run(ctx, opts) {
    if (typeof opts.agent !== "string" || !opts.agent) return ctx.fail({ error: "draft({ agent }) needs the name of an agent" });
    const previous = object(ctx.output(ctx.step));
    const input = defined({ prompt: await asked(ctx, opts), session: previous.session });
    const result = await ctx.agent(opts.agent, input, agentOptions(opts, { mode: opts.mode ?? "plan", inPlace: true }));
    const output = object(result.output);
    const draft = String(output.draft ?? result.text ?? "");
    return ctx.pass(defined({ session: result.session, draft, fields: Object.keys(output).length > 0 ? output : undefined }));
  },
});
