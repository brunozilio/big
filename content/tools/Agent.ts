const GENERAL = "general-purpose";
const COLLECT = "Use TaskOutput to collect the report before completing the request.";

type Known = { name: string; description: string; background?: boolean };

type Input = {
  description?: string;
  prompt?: string;
  name?: string;
  subagent_type?: string;
  run_in_background?: boolean;
  isolation?: string;
};

type Answer = { content: string; isError: boolean };

function known(agents: unknown): Known[] {
  return Array.isArray(agents) ? agents : [];
}

export const spec = {
  name: "Agent",
  input: {
    "properties": {
      "description": {
        "description": "A short description of the task, 3 to 5 words.",
        "type": "string"
      },
      "isolation": {
        "description": "worktree: work in a git worktree of its own, kept when it changed something. Only for an agent that edits files; outside a git repository with a commit the agent runs without it.",
        "enum": [
          "worktree"
        ]
      },
      "max_turns": {
        "maximum": 200,
        "minimum": 1,
        "type": "integer"
      },
      "model": {
        "description": "A model for this agent: an id or short name your own provider serves. One it does not serve is ignored and the agent's own or yours is used, as when left out.",
        "type": "string"
      },
      "name": {
        "description": "Only to start a long-lived teammate, never for a one-off task: a teammate runs in its own process in your team, keeps working across your turns, reports to you with SendMessage and waits idle for your next message until you end it. Leave this out for an ordinary sub-agent; an empty or placeholder name is ignored. Letters, digits, `_` and `-`.",
        "type": "string"
      },
      "team_name": {
        "description": "Ignored: your session has one team.",
        "type": "string"
      },
      "prompt": {
        "description": "The task for the agent to carry out.",
        "type": "string"
      },
      "run_in_background": {
        "description": "Run concurrently and return a task_id. Collect its report with TaskOutput.",
        "type": "boolean"
      },
      "subagent_type": {
        "description": "The kind of agent; general-purpose when left out.",
        "type": "string"
      }
    },
    "required": [
      "description",
      "prompt"
    ],
    "type": "object"
  },
  description: (ctx: { brand: { DIR: string }; agents?: Known[] }) => {
    let text = `Starts a sub-agent for a task of several steps. Each kind of agent has its own abilities and tools. Leaving out \`subagent_type\` starts a fresh general-purpose agent; \`fork\` forks you, inheriting your whole conversation and your model.

When to use it: a fresh agent costs more than it looks. It knows only what you put in \`prompt\`, and you see only the report it sends back: both handoffs drop detail, and neither of you can tell what the other missed. You cannot watch it work, only wait or cancel. Its mistakes come back in the same confident tone as its findings, and an agent handed your hypothesis tends to return it confirmed. Several at once spend tokens in a burst the user did not ask for.

Reach for it when you have independent work to run in parallel, when the user asks for a side task that should not block you, or when answering means reading across many files: delegate that and you keep the conclusion, not the file dumps. Do the work yourself when it is a handful of tool calls or a lookup whose target you already know. Delegate a review only when you want a reading not anchored on yours, and then give it the code, not your conclusion. Once you delegated something, do not also do it yourself; wait for the result. When in doubt, do not start one.

Brief it like the peer it is: state the goal and what you already ruled out, point it at the files and documents worth reading instead of retyping them, and keep the scope explicit and narrow. If you cannot write a clear brief, you do not understand the task well enough to hand it off. A fork needs no background, since it has yours: give it only its directive.

- Its report is not shown to the user: pass on what matters.
- \`run_in_background\` runs it while you go on; collect its report with TaskOutput. Never make up or predict a pending agent's result; if the user asks before it arrives, say it is still running.
- Each kind's model, effort and tools come from its definition (\`${ctx.brand.DIR}/agents/*.ts\` \`spec\`, or \`*.md\` frontmatter).`;
    text += "\n\nAgents available through `subagent_type`:\n- general-purpose: researching complex questions, searching for code and carrying out tasks of several steps (the default).";
    for (const agent of known(ctx.agents)) {
      text += `\n- ${agent.name}: ${agent.description}`;
    }
    return text;
  },
};

export function effects(_input: Input | null) {
  return [{ spawn: "agent" }];
}

export function summary(input: Input | null) {
  return typeof input?.description === "string" ? input.description : "";
}

function inBackground(input: Input, agents: Known[]) {
  if (typeof input.run_in_background === "boolean") {
    return input.run_in_background;
  }
  const kind = input.subagent_type;
  if (typeof kind !== "string" || kind === "" || kind === GENERAL) {
    return false;
  }
  return agents.find((agent) => agent.name === kind)?.background === true;
}

function teammate(started: { name: string; team: string }) {
  const { name, team } = started;
  return `Started teammate ${name} in team ${team}. It works on its own, in its own process, and reports to you with SendMessage; its messages reach you between your steps, and it tells you when it goes idle. Send it more work with SendMessage (to: "${name}"), and end it with SendMessage (to: "${name}", message: {"type": "shutdown_request"}) when the work is done.`;
}

function asked(input: Input, mode: unknown): Input {
  if (mode !== "plan" || input.isolation === undefined) {
    return input;
  }
  const { isolation: _, ...readOnly } = input;
  return readOnly;
}

export function execute(given: Input, ctx) {
  const input = asked(given, ctx.mode);
  if (typeof input.name === "string" && input.name.trim() !== "") {
    const started: Answer | { name: string; team: string } = ctx.agent.teammate(input);
    return "content" in started ? started : teammate(started);
  }
  if (!inBackground(input, known(ctx.agents))) {
    return ctx.agent.spawn(input);
  }
  const started: Answer | { task_id: string } = ctx.agent.background(input);
  if ("content" in started) {
    return started;
  }
  return JSON.stringify({ instructions: COLLECT, status: "running", task_id: started.task_id });
}
