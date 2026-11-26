import { listIn, listOf, optionalFlag, optionalNumber, optionalText, text, textAt } from "sdk/connectors/index.ts";
import { type Api, get, items, post } from "./graph.ts";

const MESSAGE_FIELDS = "id,subject,from,receivedDateTime,toRecipients,bodyPreview,isRead,webLink";

const topOf = (limit?: number) => (limit && limit > 0 ? Math.min(limit, 50) : 25);

function addresses(list: unknown[]): { emailAddress: { address: string } }[] {
  return list.map((who) => ({ emailAddress: { address: String(who) } })).filter((who) => who.emailAddress.address !== "");
}

function messageOf(raw: any) {
  return {
    ref: String(raw?.id ?? ""),
    subject: String(raw?.subject ?? ""),
    from: textAt(raw, ["from", "emailAddress", "address"]),
    received: String(raw?.receivedDateTime ?? ""),
    to: listIn(raw?.toRecipients).map((who) => textAt(who, ["emailAddress", "address"])),
    preview: String(raw?.bodyPreview ?? ""),
    body: textAt(raw, ["body", "content"]),
    unread: raw?.isRead === false,
    url: String(raw?.webLink ?? ""),
  };
}

function listQuery(args: Record<string, unknown>): string {
  const top = topOf(optionalNumber(args, "limit"));
  const query = optionalText(args, "query");
  const unread = optionalFlag(args, "unread") ?? false;
  const select = `$select=${MESSAGE_FIELDS}&$top=${top}`;
  if (query && query !== "") return `?$search="${encodeURIComponent(query)}"&${select}`;
  if (unread) return `?$filter=isRead eq false&$orderby=receivedDateTime desc&${select}`;
  return `?$orderby=receivedDateTime desc&${select}`;
}

export function listMessages(api: Api, args: Record<string, unknown>) {
  const folder = optionalText(args, "folder");
  const base = folder && folder !== "" ? `/me/mailFolders/${encodeURIComponent(folder)}/messages` : "/me/messages";
  return items(get(api, `${base}${listQuery(args)}`)).map(messageOf);
}

export function message(api: Api, id: string) {
  return messageOf(get(api, `/me/messages/${encodeURIComponent(id)}?$select=${MESSAGE_FIELDS},body`));
}

export function send(api: Api, args: Record<string, unknown>) {
  const html = optionalFlag(args, "html") ?? false;
  const body = {
    message: {
      subject: text(args, "subject"),
      body: { contentType: html ? "HTML" : "Text", content: text(args, "body") },
      toRecipients: addresses(listOf(args, "to")),
      ccRecipients: addresses(listOf(args, "cc")),
    },
    saveToSentItems: true,
  };
  post(api, "/me/sendMail", body);
  return { sent: true };
}

export function reply(api: Api, args: Record<string, unknown>) {
  const id = encodeURIComponent(text(args, "message"));
  const path = (optionalFlag(args, "all") ?? false) ? `/me/messages/${id}/replyAll` : `/me/messages/${id}/reply`;
  post(api, path, { comment: text(args, "body") });
  return { sent: true };
}
