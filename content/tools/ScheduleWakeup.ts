const WAKE = { least: 60, most: 3600 };

export const spec = {
  name: "ScheduleWakeup",
  input: {
    "additionalProperties": false,
    "properties": {
      "delaySeconds": {
        "type": "number"
      },
      "prompt": {
        "type": "string"
      },
      "reason": {
        "description": "Why this delay, in one sentence.",
        "type": "string"
      }
    },
    "required": [
      "delaySeconds",
      "prompt"
    ],
    "type": "object"
  },
  description: `Send a prompt to this session once, after a delay (60 to 3600 seconds), to pick up work later: check on a long build, a deploy, or repeat a task at your own pace by scheduling the next wakeup each time. It fires only while the session is idle. Each wakeup is a model turn: to wait until a tool's result meets a fixed condition (an issue reaching a status, a file changing), use Watch, which checks without you; to follow a command's output, Monitor.`,
};

type Input = { delaySeconds: number; prompt: string; reason?: string };

export function effects(_input: Input | null) {
  return [{ spawn: "schedule" }];
}

export function summary(input: Input | null) {
  return typeof input?.reason === "string" ? input.reason : "";
}

function seconds(delay: number) {
  const whole = Number.isNaN(delay) ? 0 : Math.trunc(delay);
  return Math.min(Math.max(whole, WAKE.least), WAKE.most);
}

export function execute(input: Input, ctx) {
  if (typeof input?.prompt !== "string") {
    return { content: "Missing required parameter `prompt`.", isError: true };
  }
  if (typeof input.delaySeconds !== "number") {
    return { content: "Missing required parameter `delaySeconds`.", isError: true };
  }
  const delay = seconds(input.delaySeconds);
  try {
    const scheduled = ctx.sessions.schedule({ prompt: input.prompt, after: delay });
    return `Wakeup in ${delay}s: ${scheduled.id}, at ${scheduled.local}: ${input.prompt}`;
  } catch (error) {
    return { content: String(error.message ?? error), isError: true };
  }
}
