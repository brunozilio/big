import { flag, listIn, optionalNumber, text, textAt } from "sdk/connectors/index.ts";
import { type Api, get, items, post } from "./graph.ts";

const topOf = (limit?: number) => (limit && limit > 0 ? Math.min(limit, 50) : 20);

function memberNames(raw: any): string[] {
  return listIn(raw?.members).map((who) => String(who?.displayName ?? who?.email ?? "")).filter((name) => name !== "");
}

function chatOf(raw: any) {
  return {
    ref: String(raw?.id ?? ""),
    kind: String(raw?.chatType ?? "oneOnOne"),
    topic: String(raw?.topic ?? ""),
    members: memberNames(raw),
    updated: String(raw?.lastUpdatedDateTime ?? ""),
    url: String(raw?.webUrl ?? ""),
  };
}

function messageOf(raw: any) {
  return {
    ref: String(raw?.id ?? ""),
    from: textAt(raw, ["from", "user", "displayName"]),
    sent: String(raw?.createdDateTime ?? ""),
    text: textAt(raw, ["body", "content"]),
    url: String(raw?.webUrl ?? ""),
  };
}

export function listChats(api: Api, args: Record<string, unknown>) {
  const top = topOf(optionalNumber(args, "limit"));
  return items(get(api, `/me/chats?$expand=members&$top=${top}`)).map(chatOf);
}

export function listMessages(api: Api, args: Record<string, unknown>) {
  const id = encodeURIComponent(text(args, "chat"));
  const top = topOf(optionalNumber(args, "limit"));
  return items(get(api, `/me/chats/${id}/messages?$top=${top}`)).map(messageOf);
}

export function sendMessage(api: Api, args: Record<string, unknown>) {
  const id = encodeURIComponent(text(args, "chat"));
  const html = flag(args, "html");
  const made = post(api, `/me/chats/${id}/messages`, { body: { contentType: html ? "html" : "text", content: text(args, "text") } }) as any;
  return { sent: true, ref: String(made?.id ?? "") };
}
