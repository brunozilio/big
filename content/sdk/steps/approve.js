import { step, actions, markdown } from "sdk";
import { defined, object, waitOptions } from "./context.js";
import { given } from "./given.js";

const APPROVE = "approve";
const REJECT = "reject";

function shown(s) {
  const parts = [];
  if (s.text) parts.push(markdown(s.text));
  if (s.by) parts.push(markdown(`${s.approved ? "Approved" : "Rejected"} by ${s.by}${s.note ? `: ${s.note}` : ""}`));
  return parts;
}

function viewOf(opts, text) {
  return [
    markdown(text),
    actions([
      { id: APPROVE, label: opts.approveLabel ?? "Approve", variant: "primary" },
      { id: REJECT, label: opts.rejectLabel ?? "Reject", variant: "danger" },
    ]),
  ];
}

export default step({
  name: "sign-off",
  id: "approve",
  label: "Approval",
  icon: "check",
  doing: "Waiting for approval",
  view: shown,
  async run(ctx, opts) {
    const text = String((await given(ctx, opts.text)) ?? `Approve: ${ctx.item.title ?? ""}`);
    for (;;) {
      const answer = object(await ctx.waitHuman(viewOf(opts, text), waitOptions(opts)));
      if (answer.action !== APPROVE && answer.action !== REJECT) continue;
      const approved = answer.action === APPROVE;
      const data = defined({ text, approved, by: answer.by, note: object(answer.values).note });
      if (approved) return ctx.pass(data);
      return opts.onReject ? ctx.goto(opts.onReject, data) : ctx.fail({ ...data, error: "rejected" });
    }
  },
});
