const DAY_MS = 24 * 60 * 60 * 1000;

export const spec = {
  name: "History",
  deferred: true,
  input: {
    additionalProperties: false,
    properties: {
      action: {
        description: "`search` finds words in past conversations; `sessions` lists them, newest first; `read` shows the turns of one; `places` names the folders and files this computer keeps for the app.",
        enum: ["search", "sessions", "read", "places"],
        type: "string",
      },
      text: { description: "search: the words to find; every word must appear, case and accents ignored.", type: "string" },
      cwd: { description: "search, sessions: only conversations held in this folder or below it.", type: "string" },
      days: { description: "search, sessions: only conversations touched in the last this many days.", type: "integer" },
      limit: { description: "search, sessions: how many to return (20 if omitted, at most 100).", type: "integer" },
      session: { description: "read: the session id from a search or listing (its first 8 characters are enough).", type: "string" },
      from: { description: "read: the first turn to show, from 0.", type: "integer" },
      count: { description: "read: how many turns to show (20 if omitted, at most 50).", type: "integer" },
    },
    required: ["action"],
    type: "object",
  },
  description: (ctx) => `Look into what ${ctx.brand.NAME} itself kept on this computer: past conversations (chats and work-item steps alike), across every project, including those kept under a former name of the app. Use it whenever the user mentions something said or done in an earlier session, before guessing or searching files by hand: \`search\` with a few distinctive words, then \`read\` the session around the turn it found. \`places\` names where the app keeps its data, logs and settings here.`,
};

type Input = {
  action: "search" | "sessions" | "read" | "places";
  text?: string;
  cwd?: string;
  days?: number;
  limit?: number;
  session?: string;
  from?: number;
  count?: number;
};

type Session = { session: string; kept_in: string; title?: string; cwd?: string; updated_ms: number };
type Hit = Session & { role: string; at_ms?: number; turn?: number; excerpt: string };
type Read = Session & { total: number; turns: { turn: number; role: string; text: string }[] };
type Place = { name: string; path: string; holds: string; exists: boolean };

export function offered(ctx) {
  return ctx.services?.history === true;
}

export function effects(_input: Input | null) {
  return [];
}

export function summary(input: Input | null) {
  if (!input) {
    return "";
  }
  return input.action === "read" ? `read ${input.session ?? ""}` : [input.action, input.text].filter(Boolean).join(" ");
}

function when(ms: number | undefined): string {
  return ms ? new Date(ms).toISOString().slice(0, 16).replace("T", " ") : "";
}

function named(session: Session): string {
  const where = session.kept_in === "current" ? "" : ` [kept as ${session.kept_in}]`;
  const title = session.title ? `"${session.title}"` : "untitled";
  return `${session.session.slice(0, 12)} ${title} in ${session.cwd ?? "an unknown folder"}, last ${when(session.updated_ms)}${where}`;
}

function filters(input: Input) {
  const since = input.days && input.days > 0 ? Date.now() - input.days * DAY_MS : undefined;
  return { cwd: input.cwd, since_ms: since, limit: input.limit };
}

function searched(input: Input, ctx): string {
  const hits: Hit[] = ctx.history.search({ text: input.text ?? "", ...filters(input) });
  if (hits.length === 0) {
    return "Nothing found. Try fewer or other words, drop the folder or day filters, or list the sessions.";
  }
  return hits
    .map((hit) => {
      const at = hit.at_ms ? ` at ${when(hit.at_ms)}` : "";
      const turn = hit.turn === undefined ? "" : `, turn ${hit.turn}`;
      return `${named(hit)}\n  ${hit.role}${at}${turn}: ${hit.excerpt}`;
    })
    .join("\n");
}

function listed(input: Input, ctx): string {
  const sessions: Session[] = ctx.history.sessions(filters(input));
  return sessions.length === 0 ? "No saved conversation matches." : sessions.map(named).join("\n");
}

function read(input: Input, ctx): string {
  const kept: Read = ctx.history.read({ session: input.session ?? "", from: input.from, count: input.count });
  const turns = kept.turns.map((turn) => `[${turn.turn}] ${turn.role}: ${turn.text}`);
  const last = kept.turns.length > 0 ? kept.turns[kept.turns.length - 1].turn + 1 : kept.total;
  const more = last < kept.total ? `\n… ${kept.total - last} more turns; read from ${last}.` : "";
  return `${named(kept)}, ${kept.total} turns\n${turns.join("\n")}${more}`;
}

function places(ctx): string {
  const kept: Place[] = ctx.history.places();
  return kept.map((place) => `${place.name}: ${place.path}${place.exists ? "" : " (not there)"}\n  ${place.holds}`).join("\n");
}

export function execute(input: Input, ctx) {
  try {
    switch (input.action) {
      case "search":
        return searched(input, ctx);
      case "sessions":
        return listed(input, ctx);
      case "read":
        return read(input, ctx);
      case "places":
        return places(ctx);
      default:
        return { content: `Unknown action ${input.action}.`, isError: true };
    }
  } catch (error) {
    return { content: String(error.message ?? error), isError: true };
  }
}
