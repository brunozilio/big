import { listIn, listOf, optionalFlag, optionalNumber, optionalText, text, textAt } from "sdk/connectors/index.ts";
import { type Api, get, items, post } from "./graph.ts";

const EVENT_FIELDS = "id,subject,start,end,location,organizer,attendees,onlineMeeting,webLink,isOnlineMeeting";

const topOf = (limit?: number) => (limit && limit > 0 ? Math.min(limit, 50) : 25);

const moment = (value: string) => ({ dateTime: value, timeZone: "UTC" });

function eventOf(raw: any) {
  return {
    ref: String(raw?.id ?? ""),
    subject: String(raw?.subject ?? ""),
    start: textAt(raw, ["start", "dateTime"]),
    end: textAt(raw, ["end", "dateTime"]),
    location: textAt(raw, ["location", "displayName"]),
    organizer: textAt(raw, ["organizer", "emailAddress", "address"]),
    attendees: listIn(raw?.attendees).map((who) => textAt(who, ["emailAddress", "address"])),
    online: textAt(raw, ["onlineMeeting", "joinUrl"]),
    url: String(raw?.webLink ?? ""),
  };
}

export function listEvents(api: Api, args: Record<string, unknown>) {
  const top = topOf(optionalNumber(args, "limit"));
  const from = optionalText(args, "from");
  const to = optionalText(args, "to");
  const select = `$select=${EVENT_FIELDS}&$top=${top}`;
  if (from && from !== "" && to && to !== "") {
    const window = `startDateTime=${encodeURIComponent(from)}&endDateTime=${encodeURIComponent(to)}`;
    return items(get(api, `/me/calendarView?${window}&$orderby=start/dateTime&${select}`)).map(eventOf);
  }
  return items(get(api, `/me/events?$orderby=start/dateTime desc&${select}`)).map(eventOf);
}

export function createEvent(api: Api, args: Record<string, unknown>) {
  const online = optionalFlag(args, "online") ?? false;
  const location = optionalText(args, "location");
  const body = {
    subject: text(args, "subject"),
    start: moment(text(args, "start")),
    end: moment(text(args, "end")),
    attendees: listOf(args, "attendees").map((who) => ({ emailAddress: { address: String(who) }, type: "required" })),
    body: { contentType: "Text", content: optionalText(args, "body") ?? "" },
    location: location && location !== "" ? { displayName: location } : undefined,
    isOnlineMeeting: online,
    onlineMeetingProvider: online ? "teamsForBusiness" : undefined,
  };
  const made = post(api, "/me/events", body) as any;
  return { ref: String(made?.id ?? ""), url: String(made?.webLink ?? ""), online: textAt(made, ["onlineMeeting", "joinUrl"]), created: true };
}
