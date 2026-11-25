export const spec = {
  name: "ExitPlanMode",
  input: {
    "additionalProperties": false,
    "properties": {
      "plan": {
        "description": "The plan, in markdown.",
        "type": "string"
      }
    },
    "required": [
      "plan"
    ],
    "type": "object"
  },
  description: `In planning mode, present your finished plan to the user for approval. The user reads it and either approves it, which ends planning so you start implementing it right away, or keeps planning and tells you what to change. Pass the whole plan, in markdown, as \`plan\`; do not also write it as a message. Use it only when the task needs code changes and the plan is complete; to clarify requirements or choose between approaches, ask with AskUserQuestion first. Never ask in text whether the plan is fine: this tool is that question.`,
};

type Input = { plan?: unknown };
type State = { main: boolean; planning: boolean; prompts: boolean };
type Answer =
  | { approved: string }
  | { handedOff: string }
  | { kept: true; note?: string }
  | { stopped: true };

export function effects(_input: Input | null) {
  return [];
}

export function summary(_input: Input | null) {
  return "";
}

function failed(content: string) {
  return { content, isError: true };
}

function refusal(state: State) {
  if (!state.main) {
    return "Only the main agent leaves plan mode.";
  }
  if (!state.planning) {
    return "You are not in plan mode: there is no plan to approve. Go on with the task.";
  }
  if (!state.prompts) {
    return "The plan cannot be put to the user here. Present it as your final message.";
  }
  return null;
}

function kept(note: string | undefined) {
  const rest = typeof note === "string"
    ? `They said: ${note}`
    : "Stop here and wait for them to say what to change.";
  return failed(`The user wants to keep planning and did not approve the plan. Do not implement anything. ${rest}`);
}

export function execute(input: Input, ctx) {
  const refused = refusal(ctx.plan.state());
  if (refused !== null) {
    return failed(refused);
  }
  const plan = typeof input?.plan === "string" ? input.plan.trim() : "";
  if (plan === "") {
    return failed("Missing the plan: pass it, in markdown, as `plan`.");
  }
  const answer: Answer = ctx.plan.exit({ plan });
  if ("handedOff" in answer) {
    return answer.handedOff;
  }
  if ("stopped" in answer) {
    return failed(
      "Not run: the run was stopped while waiting for the user's answer, so the user never saw the plan and nothing was changed. Call this tool again with the same plan and wait for the answer. Never decide for the user.",
    );
  }
  if ("kept" in answer) {
    return kept(answer.note);
  }
  return `The user approved your plan and planning is over. Start implementing it now, following the plan. Track the steps with the task tools if there are several.`;
}
