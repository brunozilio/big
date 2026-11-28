export const spec = {
  name: "AskUserQuestion",
  input: {
    "properties": {
      "questions": {
        "items": {
          "properties": {
            "header": {
              "description": "A tag of at most 12 characters.",
              "type": "string"
            },
            "multiSelect": {
              "default": false,
              "type": "boolean"
            },
            "options": {
              "items": {
                "properties": {
                  "description": {
                    "type": "string"
                  },
                  "label": {
                    "type": "string"
                  }
                },
                "required": [
                  "label",
                  "description"
                ],
                "type": "object"
              },
              "maxItems": 4,
              "minItems": 2,
              "type": "array"
            },
            "question": {
              "description": "The whole question, ending with a question mark.",
              "type": "string"
            }
          },
          "required": [
            "question",
            "header",
            "options",
            "multiSelect"
          ],
          "type": "object"
        },
        "maxItems": 4,
        "minItems": 1,
        "type": "array"
      }
    },
    "required": [
      "questions"
    ],
    "type": "object"
  },
  description: `Ask the user one to four questions, each with two to four options, when you cannot go on without a decision that is genuinely theirs. The user can also answer in their own words. If you recommend an option, put it first and end its label with "(Recommended)". Every option, the recommended one above all, is a concrete decision you can act on; never an option like "I will provide it", since the user can always answer in their own words. Once answered, go on with the answer and do not ask the same thing again. Ask only when the answer changes what you do next: not for a choice with a conventional default nor for a fact you can check in the code yourself; then pick the obvious option, say so and go on. The user may see only the question, so make each one stand on its own. In planning mode, ask to clarify requirements or choose between approaches, never whether the plan is ready or whether to proceed.`,
  profile: { effects: ["read"], risk: "low", reversible: true, untrusted: false, sensitive: false },
};

const MAX_QUESTIONS = 4;
const MAX_OPTIONS = 4;
const SHOWN = 80;

type Option = { label?: unknown; description?: unknown };
type Asked = { question?: unknown; header?: unknown; options?: Option[]; multiSelect?: unknown };
type Input = { questions?: Asked[] };
type Question = { header: string; text: string; options: { label: string; description: string }[]; multi: boolean };
type Answer =
  | { answered: number[][] }
  | { declined: true; note?: string }
  | { stopped: true };

export function effects(_input: Input | null) {
  return [];
}

export function summary(input: Input | null) {
  const first = Array.isArray(input?.questions) ? input.questions[0]?.question : undefined;
  return typeof first === "string" ? Array.from(first).slice(0, SHOWN).join("") : "";
}

function question(asked: Asked): Question {
  const options = (Array.isArray(asked?.options) ? asked.options : [])
    .slice(0, MAX_OPTIONS)
    .filter((option) => typeof option?.label === "string")
    .map((option) => ({
      label: option.label as string,
      description: typeof option.description === "string" ? option.description : "",
    }));
  if (typeof asked?.question !== "string") {
    throw new Error("every question needs its `question`");
  }
  if (options.length < 2) {
    throw new Error(`"${asked.question}" needs at least two options`);
  }
  return {
    header: typeof asked.header === "string" ? asked.header : "",
    text: asked.question,
    options,
    multi: asked.multiSelect === true,
  };
}

function questions(input: Input): Question[] {
  if (!Array.isArray(input?.questions) || input.questions.length === 0) {
    throw new Error("`questions` must list at least one");
  }
  return input.questions.slice(0, MAX_QUESTIONS).map(question);
}

function answers(asked: Question[], picks: number[][]) {
  const lines = asked.map((question, at) => {
    const chosen = (picks[at] ?? []).map((pick) => question.options[pick]?.label).filter((label) => label !== undefined);
    const answer = chosen.length === 0 ? "(no answer)" : chosen.join(", ");
    return `"${question.text}" = "${answer}"`;
  });
  return `The user answered: ${lines.join("; ")}. Go on with these answers in mind.`;
}

function failed(content: string) {
  return { content, isError: true };
}

export function execute(input: Input, ctx) {
  if (!ctx.user.prompts()) {
    return failed("Interactive questions are disabled for this agent.");
  }
  let asked: Question[];
  try {
    asked = questions(input);
  } catch (error) {
    return failed(`The questions could not be asked: ${error.message ?? error}.`);
  }
  const answer: Answer = ctx.user.ask({ questions: asked });
  if ("stopped" in answer) {
    return failed(
      "Not run: the run was stopped while waiting for the user's answer, so the user never saw the question and nothing was changed. Call this tool again with the same questions and wait for the answer. Never decide for the user.",
    );
  }
  if ("declined" in answer) {
    return typeof answer.note === "string"
      ? failed(`The user declined to answer the questions and said instead: ${answer.note}`)
      : failed("The user declined to answer. Do not ask again; go on with your best judgement and say what you assumed.");
  }
  return answers(asked, answer.answered);
}
