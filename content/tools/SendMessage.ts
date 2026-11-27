export const spec = {
  name: "SendMessage",
  topLevelOnly: true,
  input: {
    "properties": {
      "message": {
        "anyOf": [
          {
            "description": "What to tell or ask the agent.",
            "type": "string"
          },
          {
            "description": "A team protocol message: {type: \"shutdown_request\", reason?}, {type: \"shutdown_response\", request_id, approve, reason?} or {type: \"plan_approval_response\", request_id, approve, feedback?}.",
            "properties": {
              "approve": { "type": "boolean" },
              "feedback": { "type": "string" },
              "reason": { "type": "string" },
              "request_id": { "type": "string" },
              "type": { "enum": ["shutdown_request", "shutdown_response", "plan_approval_response"] }
            },
            "required": ["type"],
            "type": "object"
          }
        ]
      },
      "summary": {
        "description": "For a teammate: what the message is about in 5 to 10 words, shown before it is read.",
        "type": "string"
      },
      "to": {
        "description": "The agent_id an Agent report gave (agent-1), a teammate's name, team-lead (or main) from a teammate, or another session's name (ListSessions).",
        "type": "string"
      }
    },
    "required": [
      "to",
      "message"
    ],
    "type": "object"
  },
  description: `Continues a sub-agent you started earlier in this request, with its conversation intact: it remembers its task, what it read and what it reported. \`to\` is the agent_id at the end of its report. Use it for a follow-up, a correction or the next step of the same work, instead of starting a fresh agent and briefing it again. The agent answers with a new report, which, like the first, is not shown to the user.

- An agent still running in the background cannot take a message: collect it with TaskOutput first.
- Agents last for this request only; start a new one for later requests.
- \`to\` can also name another of the user's open sessions (ListSessions lists them): the message waits in its inbox and reaches its agent; any answer comes back to you as an event.`,
};

type Input = { to?: string; message?: unknown; summary?: string };

type Answer = { content: string; isError: boolean };

export function effects(_input: Input | null) {
  return [{ spawn: "agent" }];
}

export function summary(input: Input | null) {
  return typeof input?.to === "string" ? input.to : "";
}

function failed(content: string): Answer {
  return { content, isError: true };
}

function toTheTeam(input: Input, ctx) {
  const told: null | { sent: string } | { problem: string } = ctx.peers.team(input);
  if (told === null) {
    return null;
  }
  return "sent" in told ? `Sent to ${told.sent}.` : failed(`The message was not sent: ${told.problem}.`);
}

export function execute(input: Input, ctx) {
  const team = toTheTeam(input, ctx);
  if (team !== null) {
    return team;
  }
  const session: Answer | null = ctx.peers.session(input);
  if (session !== null) {
    return session;
  }
  const to = typeof input.to === "string" ? input.to.trim() : "";
  if (to === "") {
    return failed("Missing required parameter `to`.");
  }
  if (typeof input.message !== "string" || input.message.trim() === "") {
    return failed("Missing required parameter `message`.");
  }
  return ctx.agent.message({ to, message: input.message });
}
