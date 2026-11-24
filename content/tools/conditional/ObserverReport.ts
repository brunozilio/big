export const spec = {
  name: "ObserverReport",
  input: {
    "additionalProperties": false,
    "properties": {
      "message": {
        "description": "What the agent you watch should know, in a few plain sentences: what you saw and why it matters.",
        "type": "string"
      }
    },
    "required": [
      "message"
    ],
    "type": "object"
  },
  description: `Sends one note to the agent you watch. It reaches that agent between its steps as advice from its observer, never as the user's words. Use it only for something worth interrupting for: a mistake about to compound, a constraint the agent missed, prior art it does not know about. Most of the time, say nothing.`,
};

type Input = { message?: string };

export function effects(_input: Input | null) {
  return [];
}

export function summary(input: Input | null) {
  return typeof input?.message === "string" ? input.message.split("\n")[0].replace(/\r$/, "") : "";
}

export function execute(input: Input, ctx) {
  const message = typeof input?.message === "string" ? input.message.trim() : "";
  if (message === "") {
    return { content: "Missing required parameter `message`.", isError: true };
  }
  ctx.observer.report({ message });
  return "Sent to the agent you watch.";
}
