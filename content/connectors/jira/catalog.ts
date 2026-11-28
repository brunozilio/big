import { type Language, pick } from "sdk/connectors/index.ts";
import { FLAG, MAYBE_TEXT, TEXT, event } from "sdk/connectors/events.ts";

export const CREATED = "jira.issue.created";
export const COMMENTED = "jira.comment.added";
export const MOVED = "jira.status.changed";
export const FLAGGED = "jira.impediment.changed";

export function events({ language }: { language: Language }) {
  return [
    event(CREATED, {
      label: pick(language, "Issue created", "Issue creada", "Issue criada"),
      capability: "tracker",
      source: "issue",
      aliases: ["tracker.issue.created"],
      properties: { ref: TEXT, title: TEXT, status: TEXT, type: TEXT, parent: TEXT },
      required: ["ref", "title", "status"],
      filters: ["type", "status"],
    }),
    event(COMMENTED, {
      label: pick(language, "Comment added", "Comentario añadido", "Comentário adicionado"),
      capability: "tracker",
      source: "issue",
      aliases: ["tracker.comment.added"],
      properties: { id: TEXT, author: {}, body: TEXT, at: TEXT },
      required: ["id", "body"],
    }),
    event(MOVED, {
      label: pick(language, "Status changed", "Estado cambiado", "Status alterado"),
      capability: "tracker",
      source: "issue",
      aliases: ["tracker.status.changed"],
      properties: { from: MAYBE_TEXT, to: MAYBE_TEXT, author: TEXT },
      required: ["from", "to"],
      filters: ["from", "to"],
    }),
    event(FLAGGED, {
      label: pick(language, "Impediment changed", "Impedimento cambiado", "Impedimento alterado"),
      capability: "tracker",
      source: "issue",
      aliases: ["tracker.impediment.changed"],
      properties: { from: FLAG, to: FLAG, author: TEXT },
      required: ["from", "to"],
      filters: ["to"],
    }),
  ];
}
