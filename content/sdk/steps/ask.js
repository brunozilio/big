import { step, markdown } from "sdk";
import { attempt, defined, effects, object } from "./context.js";

const UNANSWERED = "No assistant is connected, so the item continues without asking.";

function questionOf(ctx, opts) {
  const given = typeof opts.question === "function" ? opts.question(ctx.item, ctx) : opts.question;
  return String(given ?? ctx.item.description ?? ctx.item.title ?? "").trim();
}

function contextOf(ctx, opts) {
  const given = typeof opts.context === "function" ? opts.context(ctx.item, ctx) : opts.context;
  return given === undefined || given === null ? undefined : String(given);
}

function shown(s) {
  if (!s.answer) return s.reason ? [markdown(s.reason)] : [];
  const sources = Array.isArray(s.sources) && s.sources.length > 0 ? `\n\n${s.sources.map((found) => `- [${found.title}](${found.url})`).join("\n")}` : "";
  return [markdown(`${s.answer}${sources}`)];
}

export default step({
  name: "ask",
  label: "Ask",
  icon: "sPlan",
  doing: "Asking",
  view: shown,
  async run(ctx, opts) {
    const question = questionOf(ctx, opts);
    if (question === "") return ctx.skip({ reason: "There is nothing to ask." });
    const previous = object(ctx.output(ctx.step));
    const args = defined({ question, context: contextOf(ctx, opts), mode: opts.mode, thread: opts.thread ?? previous.thread });
    const work = () => ctx.call("assistant", "ask", args, effects(opts));
    const answered = opts.optional === false ? await work() : await attempt(work);
    if (!answered) return ctx.skip({ reason: UNANSWERED });
    return ctx.pass(defined({ question, answer: answered.answer, sources: answered.sources ?? [], thread: answered.thread, model: answered.model }));
  },
});
