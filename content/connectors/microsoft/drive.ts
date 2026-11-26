import { optionalNumber, optionalText, text } from "sdk/connectors/index.ts";
import { type Api, download, get, items, quote } from "./graph.ts";

const topOf = (limit?: number) => (limit && limit > 0 ? Math.min(limit, 100) : 50);

const driveBase = (drive?: string) => (drive && drive !== "" ? `/drives/${encodeURIComponent(drive)}` : "/me/drive");

function itemOf(raw: any) {
  return {
    ref: String(raw?.id ?? ""),
    name: String(raw?.name ?? ""),
    kind: raw?.folder ? "folder" : "file",
    size: Number(raw?.size ?? 0),
    modified: String(raw?.lastModifiedDateTime ?? ""),
    url: String(raw?.webUrl ?? ""),
    path: String(raw?.parentReference?.path ?? ""),
    mime: raw?.file?.mimeType ?? "",
  };
}

export function list(api: Api, args: Record<string, unknown>) {
  const base = driveBase(optionalText(args, "drive"));
  const folder = optionalText(args, "folder");
  const top = topOf(optionalNumber(args, "limit"));
  const where = folder && folder !== "" ? `${base}/root:/${encodeURI(folder)}:/children` : `${base}/root/children`;
  return items(get(api, `${where}?$top=${top}`)).map(itemOf);
}

export function search(api: Api, args: Record<string, unknown>) {
  const base = driveBase(optionalText(args, "drive"));
  const top = topOf(optionalNumber(args, "limit"));
  const query = quote(text(args, "query"));
  return items(get(api, `${base}/root/search(q='${encodeURIComponent(query)}')?$top=${top}`)).map(itemOf);
}

export function downloadFile(api: Api, args: Record<string, unknown>) {
  const base = driveBase(optionalText(args, "drive"));
  const id = encodeURIComponent(text(args, "file"));
  const meta = get(api, `${base}/items/${id}?$select=name,file`) as any;
  const content = download(api, `${base}/items/${id}/content`);
  return { name: String(meta?.name ?? ""), mime: meta?.file?.mimeType ?? content.mime, base64: content.base64, truncated: content.truncated };
}

export function sites(api: Api, args: Record<string, unknown>) {
  const query = optionalText(args, "query") ?? "";
  const found = query === "" ? get(api, "/sites?search=*") : get(api, `/sites?search=${encodeURIComponent(query)}`);
  return items(found).map((raw: any) => ({
    ref: String(raw?.id ?? ""),
    name: String(raw?.displayName ?? raw?.name ?? ""),
    url: String(raw?.webUrl ?? ""),
    host: String(raw?.siteCollection?.hostname ?? ""),
  }));
}
