import { step, form, markdown } from "sdk";
import { defined, object, waitOptions } from "./context.js";
import { fieldsText, given } from "./given.js";

const ANSWER = [{ name: "answer", label: "Answer", kind: "textarea", required: true }];

function shown(s) {
  const parts = s.text ? [markdown(s.text)] : [];
  const answered = fieldsText(s.values);
  if (answered) parts.push(markdown(answered));
  return parts;
}

function missing(fields, values) {
  const empty = (value) => value === undefined || value === null || String(value).trim() === "";
  return fields.filter((field) => field.required && empty(values[field.name]));
}

export default step({
  name: "inquiry",
  id: "request-info",
  label: "Information",
  icon: "sPlan",
  doing: "Waiting for information",
  view: shown,
  async run(ctx, opts) {
    const text = String((await given(ctx, opts.text)) ?? `Information for: ${ctx.item.title ?? ""}`);
    const listed = await given(ctx, opts.fields);
    const fields = Array.isArray(listed) && listed.length > 0 ? listed : ANSWER;
    const view = [markdown(text), form(fields, { submit: opts.submitLabel ?? "Send" })];
    for (;;) {
      const answer = object(await ctx.waitHuman(view, waitOptions(opts)));
      const values = object(answer.values);
      if (missing(fields, values).length > 0) continue;
      return ctx.pass(defined({ text, values, by: answer.by }));
    }
  },
});
