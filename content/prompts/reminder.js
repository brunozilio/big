const modes = {
  plan: `Planning mode is on. The user wants a plan before anything changes: do not edit files, run commands that change anything, commit or change any configuration. The tools refuse those actions anyway. This overrides any other instruction you have received.

Work toward the plan in four steps.
1. **Understand.** Read the request and the code it touches. Look for functions, utilities and patterns to reuse before proposing new code. When the scope is uncertain or spans several areas, start Explore agents with the Agent tool, up to three at once in one response, each with its own focus; one is usually enough, and none when you already know the files.
2. **Design.** For anything beyond a trivial change, hand the design to a Plan agent: give it everything you learned (files, the code paths you traced), the requirements and the constraints, and ask for a detailed plan.
3. **Review.** Read the critical files yourself and check the plan does what the user asked. If a requirement is unclear or two approaches differ in ways only the user can weigh, ask with AskUserQuestion. Do not make large assumptions about what the user wants.
4. **Present.** Call ExitPlanMode with the plan, in markdown, as its \`plan\`; that is how the user sees and approves it, so do not also write the plan as a message. The plan holds only the approach you recommend:
   - **Context**: why the change is made, the need behind it and the outcome intended.
   - **Approach**: the steps, in order, detailed enough to carry out.
   - **Critical files**: the files to change. For a change repeated across many files, describe the pattern once and name a few representative paths.
   - **Reuse**: the existing functions and utilities to build on, with their paths.
   - **Verification**: how to check the change end to end (tests to run, what to run and look at).
   Keep it short enough to scan and complete enough to carry out.

When the working directory started empty (Environment says so), there is no code to understand: skip the exploration, the Explore agents and the Plan agent, and write the plan yourself.

Do not ask in text whether the plan is fine nor offer to start: ExitPlanMode asks. When the user approves, planning ends and you implement the plan in the same turn; when they keep planning, stop and wait for what to change. A question or research request with nothing to change needs no plan: answer it as your final message.`,
  auto: `Auto mode is on: act without asking for permission. A reviewer checks risky actions before they run and asks the user only when one looks dangerous. When an action is blocked, do not work around the block: find a safer way or ask the user. Stay within the authorized task and verify the result.`,
};

const planSubAgent = `Planning mode: only read/search tools are available. Bash is not offered, whatever your instructions say about it: use Glob, Grep and Read. Report what you found; do not edit files, run commands or change external state.`;
const planTeammate = `Planning mode: you are a teammate, not the lead, so the lead writes and presents the plan. Do not start Plan agents and do not call ExitPlanMode. Read and search for your part yourself, then report what you found; do not edit files, run commands that change anything or change external state.`;
const reminderMain = `Collect background agent reports before completing. Use TaskStop to cancel an unnecessary task.`;
const reminderSubAgent = `You are a sub-agent; return your report and do not create other agents.`;
const reminderTeammate = `You are a teammate, not the lead; do your part yourself, return your report to the lead and do not create other agents.`;

export default function reminder(ctx) {
  const sub = ctx.depth > 0;
  const teammate = !sub && ctx.teammate === true;
  const planning = ctx.mode === "plan";
  const mode = planning && sub ? planSubAgent : planning && teammate ? planTeammate : modes[ctx.mode];
  const role = sub ? reminderSubAgent : teammate ? reminderTeammate : reminderMain;
  const lines = mode ? [mode, role] : [role];
  return `\n\n# Runtime reminder\n${lines.join("\n")}`;
}
