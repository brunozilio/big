import { step, actions, logs, markdown } from "sdk";
import { defined, object, tail } from "sdk/steps/context.js";

const BASE = 'main=$(git worktree list --porcelain | awk \'/^branch /{print $2; exit}\'); git merge-base HEAD "${main:-HEAD}"';

const DIFF = 'git log --oneline "$BASE"..HEAD; echo; git diff --stat "$BASE"..HEAD; echo; git diff "$BASE"..HEAD -- "$DIR" | head -n 600';

const MERGE =
  'root=$(git worktree list --porcelain | awk \'/^worktree /{print $2; exit}\'); branch=$(git rev-parse --abbrev-ref HEAD); test -z "$(git -C "$root" status --porcelain)" || { echo "a pasta principal tem mudanças sem commit"; exit 3; }; git -C "$root" merge --no-ff --no-edit "$branch"';

const INHERIT =
  'mkdir -p "$TARGET" && git diff --name-only "$BASE"..HEAD -- "$DIR" | while read -r f; do rel=${f#"$DIR"/}; [ "$rel" = factory ] && continue; mkdir -p "$TARGET/$(dirname "$rel")"; if [ -f "$f" ]; then cp "$f" "$TARGET/$rel"; else rm -f "$TARGET/$rel"; fi; echo "$rel"; done';

async function approved(ctx, autonomy, diff) {
  if (autonomy === "out") return { action: "approve", by: "factory" };
  const view = [markdown("**A fábrica propõe esta mudança nela mesma.** As avaliações passaram."), logs(diff), actions(["approve", "reject"])];
  return ctx.waitHuman(view);
}

export default step({
  name: "promote",
  label: "Promover",
  icon: "sMerge",
  doing: "Promovendo a mudança da fábrica",
  view: (s) => [logs(s.diff ?? ""), ...(s.inherited ? [markdown(`Herdado por \`${s.base}\`:\n${s.inherited}`)] : [])],
  async run(ctx, opts) {
    const factory = object(ctx.item.factory);
    const dir = factory.dir ?? ".big";
    const base = String((await ctx.exec(BASE)).stdout ?? "").trim();
    const diff = tail((await ctx.exec(DIFF, { env: { BASE: base, DIR: dir } })).output ?? "", 20000);
    const answer = await approved(ctx, opts.autonomy ?? factory.autonomy ?? "near", diff);
    if (answer.action !== "approve") return ctx.fail({ diff, error: "a mudança da fábrica foi recusada", by: answer.by });
    const merged = await ctx.exec(MERGE);
    if (merged.code !== 0) return ctx.fail({ diff, error: `não deu para promover: ${tail(merged.output ?? "", 2000)}` });
    const target = opts.inherit === false ? null : factory.base;
    if (!target) return ctx.pass(defined({ diff, by: answer.by }));
    const copied = await ctx.exec(INHERIT, { env: { BASE: base, DIR: dir, TARGET: target } });
    return ctx.pass(defined({ diff, by: answer.by, base: target, inherited: String(copied.stdout ?? "").trim() }));
  },
});
