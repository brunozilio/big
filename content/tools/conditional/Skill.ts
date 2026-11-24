export const spec = {
  name: "Skill",
  input: {
    "properties": {
      "args": {
        "description": "Optional arguments for the skill.",
        "type": "string"
      },
      "skill": {
        "description": "The skill's name, without a leading slash.",
        "type": "string"
      }
    },
    "required": [
      "skill"
    ],
    "type": "object"
  },
  description: `Loads a skill's instructions by its exact name from the list of available skills. When a skill covers the task, call this first and follow what it returns.`,
};

type Input = { skill?: unknown; args?: unknown };
type Listed = { name: string; modelInvocation: boolean };

export function effects(_input: Input | null) {
  return [];
}

function named(input: Input | null) {
  return typeof input?.skill === "string" ? input.skill.replace(/^\/+/, "") : null;
}

export function summary(input: Input | null) {
  return named(input) ?? "";
}

function failed(content: string) {
  return { content, isError: true };
}

export function execute(input: Input, ctx) {
  const name = named(input);
  if (name === null) {
    return failed("Missing required parameter `skill`.");
  }
  const skills: Listed[] = ctx.skill.list();
  const skill = skills.find((known) => known.name === name);
  if (skill === undefined) {
    return failed(`There is no skill named ${name}. Use one of the available skills.`);
  }
  if (!skill.modelInvocation) {
    return failed(`Skill ${name} requires explicit user invocation.`);
  }
  return ctx.skill.load({ name, args: typeof input.args === "string" ? input.args : "" });
}
