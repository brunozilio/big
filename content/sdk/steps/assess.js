import { step, actions, chat, form, markdown } from "sdk";
import { agentOptions, defined, inLanguage, object, waitOptions } from "./context.js";
import { draftOf, given, judged } from "./given.js";

const APPROVE = "approve";
const CHANGES = "submit";

async function byAgent(ctx, opts, text) {
  const written = await given(ctx, opts.prompt);
  const prompt = inLanguage(ctx, `${written ? String(written) : `Review this draft of: ${ctx.item.title ?? ""}`}\n\n${text}`);
  const result = await ctx.agent(opts.agent, { prompt }, agentOptions(opts, { mode: opts.mode ?? "plan", inPlace: true }));
  return judged(result);
}

function viewOf(opts, text, session) {
  return [
    ...(session ? [chat(session)] : []),
    markdown(text),
    form([{ name: "body", label: opts.commentLabel ?? "Comment", kind: "textarea" }], { submit: opts.changesLabel ?? "Request changes" }),
    actions([{ id: APPROVE, label: opts.approveLabel ?? "Approve", variant: "primary" }]),
  ];
}

async function byHuman(ctx, opts, text, session) {
  for (;;) {
    const answer = object(await ctx.waitHuman(viewOf(opts, text, session), waitOptions(opts)));
    if (answer.action === APPROVE) return { approved: true, by: answer.by };
    const body = String(object(answer.values).body ?? "").trim();
    if (answer.action === CHANGES && body) return { approved: false, body, by: answer.by };
  }
}

function sentBack(ctx, target, data) {
  return target ? ctx.goto(target, data) : ctx.fail({ ...data, error: "changes were requested" });
}

export default step({
  name: "assess",
  id: "review",
  label: "Review",
  icon: "sReview",
  doing: "Waiting for review",
  view: (s) => [...(s.session ? [chat(s.session)] : []), ...(s.draft ? [markdown(s.draft)] : []), ...(s.body ? [markdown(s.body)] : [])],
  async run(ctx, opts) {
    const of = opts.of ?? "draft";
    const { text } = draftOf(ctx, opts, of);
    const target = opts.onChanges === undefined ? of : opts.onChanges;
    let session;
    if (opts.agent) {
      const verdict = await byAgent(ctx, opts, text);
      session = verdict.session;
      const data = defined({ body: verdict.reason, by: opts.agent, session });
      if (!verdict.approved && ctx.attempt <= (opts.rounds ?? 3)) return sentBack(ctx, target, data);
      if (opts.human === false) return verdict.approved ? ctx.pass({ ...data, approved: true, draft: text }) : sentBack(ctx, null, data);
    }
    const decided = await byHuman(ctx, opts, text, session);
    const data = defined({ draft: text, body: decided.body, by: decided.by, session });
    return decided.approved ? ctx.pass({ ...data, approved: true }) : sentBack(ctx, target, data);
  },
});
