import { step, logs } from "sdk";
import { defined, object, tail } from "sdk/steps/context.js";

const LIST = 'for f in "$EVALS"/*.json; do [ -f "$f" ] || continue; printf "%s\\t" "$f"; tr -d "\\n\\r" < "$f"; echo; done';

const AT_COMMIT =
  'd=$(mktemp -d) && git worktree add --detach -q "$d" "$COMMIT" >/dev/null 2>&1 || exit 97; (cd "$d" && sh -c "$CMD"); c=$?; git worktree remove --force "$d" >/dev/null 2>&1; exit $c';

const MISSING = 97;

export function parseCases(listing) {
  const cases = [];
  const broken = [];
  for (const row of String(listing ?? "").split("\n")) {
    if (!row.trim()) continue;
    const at = row.indexOf("\t");
    const file = at < 0 ? row : row.slice(0, at);
    try {
      const found = object(JSON.parse(at < 0 ? "" : row.slice(at + 1)));
      if (typeof found.command !== "string" || !found.command) throw new Error("missing command");
      cases.push({ ...found, file, name: found.name ?? file, expect: found.expect === "pass" ? "pass" : "fail" });
    } catch (error) {
      broken.push({ file, error: String(error && error.message ? error.message : error) });
    }
  }
  return { cases, broken };
}

function judged(expect, code) {
  if (code === MISSING) return false;
  return expect === "pass" ? code === 0 : code !== 0;
}

async function runCase(ctx, opts, entry) {
  const ran = [];
  let ok = true;
  if (entry.commit) {
    const env = { COMMIT: entry.commit, CMD: entry.command };
    const result = await ctx.exec(AT_COMMIT, defined({ env, timeout: opts.timeout }));
    const passed = judged(entry.expect, result.code);
    ok = ok && passed;
    ran.push({ name: `${entry.name} @ ${String(entry.commit).slice(0, 10)} (expects ${entry.expect})`, state: passed ? "done" : "failed", seconds: null, log: tail(result.output ?? "", 3000) });
  }
  if (entry.head !== "skip") {
    const result = await ctx.exec(entry.command, defined({ timeout: opts.timeout }));
    const passed = result.code === 0;
    ok = ok && passed;
    ran.push({ name: `${entry.name} @ current (expects pass)`, state: passed ? "done" : "failed", seconds: null, log: tail(result.output ?? "", 3000) });
  }
  return { ok, ran };
}

function wanted(ctx, opts) {
  if (!opts.requireOrigin) return [];
  const data = object(object(ctx.item.factory).data);
  return (Array.isArray(data.findings) ? data.findings : []).map((finding) => object(finding).id).filter(Boolean);
}

export default step({
  name: "evals",
  label: "Evals",
  icon: "sTest",
  doing: "Running the factory evals",
  view: (s) => [logs(Array.isArray(s.ran) && s.ran.length > 0 ? s.ran : s.output ?? "No evals.")],
  async run(ctx, opts) {
    const dir = opts.dir ?? `${object(ctx.item.factory).dir ?? ".big"}/evals`;
    const listed = await ctx.exec(LIST, { env: { EVALS: dir } });
    const { cases, broken } = parseCases(listed.stdout ?? listed.output);
    const ran = broken.map((entry) => ({ name: entry.file, state: "failed", seconds: null, log: entry.error }));
    let ok = broken.length === 0;
    for (const entry of cases) {
      const result = await runCase(ctx, opts, entry);
      ok = ok && result.ok;
      ran.push(...result.ran);
    }
    const missing = wanted(ctx, opts).filter((origin) => !cases.some((entry) => entry.origin === origin));
    if (missing.length > 0) {
      ok = false;
      ran.push({ name: "new case for the finding", state: "failed", seconds: null, log: `missing a case with origin ${missing.join(", ")}` });
    }
    const data = { cases: cases.length, ran, missing };
    if (!ok) return ctx.fail({ ...data, error: "the factory evals did not pass" });
    return ctx.pass(data);
  },
});
