import { type Creds, type Language, checked, fail, json, pick, send, siteOf, text, textOf, tokenAlternative, tokenField, unknownAction } from "sdk/connectors/index.ts";
import { design } from "sdk/connectors/contracts.ts";
import { base, heldBy, userOf } from "./login.ts";

export { session } from "./login.ts";

const MOST_TEXTS = 200;
const MOST_LINES = 150;
const DEEPEST = 5;
const MOST_PICTURE = 5 * 1024 * 1024;
const KINDS = new Set(["file", "design", "proto", "board", "slides", "buzz", "make"]);

type Ctx = { config: { host: string } } & Record<string, any>;

type Api = { ctx: Ctx; headers: Record<string, string>; base: string };

export type Link = { file: string; node?: string };

function tokenFields(language: Language) {
  const help = pick(
    language,
    "In Figma, open Settings › Security › Personal access tokens and create one with the file_content:read scope.",
    "En Figma, abre Settings › Security › Personal access tokens y crea uno con el alcance file_content:read.",
    "No Figma, abra Settings › Security › Personal access tokens e crie um token com o escopo file_content:read.",
  );
  return [tokenField("token", pick(language, "Personal access token", "Token de acceso personal", "Token de acesso pessoal"), help, true)];
}

export function provider(_args: { language?: Language }, _ctx: Ctx) {
  const auth = { kind: "steps", using: "session" };
  return { schema_version: 1, name: "Figma", logo: "logo.svg", capabilities: ["design"], auth, actions: design() };
}

export function alternatives({ language }: { language: Language }, _ctx: Ctx) {
  const label = pick(language, "Use a personal access token", "Usar un token de acceso personal", "Usar token de acesso pessoal");
  return [tokenAlternative(language, "token", label, pick(language, "Figma", "de Figma", "do Figma"), tokenFields(language), "json")];
}

export function presentation({ language }: { language: Language }, _ctx: Ctx) {
  return {
    description: pick(language, "Your Figma account", "Tu cuenta de Figma", "Sua conta Figma"),
    group: "design",
    button: pick(language, "Sign in with Figma", "Entrar con Figma", "Entrar com Figma"),
    site: pick(language, "Figma", "de Figma", "do Figma"),
    choose_title: pick(language, "Figma account", "Cuenta de Figma", "Conta do Figma"),
    hidden: false,
  };
}

function headersOf(token: string): Record<string, string> {
  const held = heldBy(token);
  if (held !== null) return { cookie: held.cookies, accept: "application/json", "x-csrf-bypass": "yes" };
  try {
    const found = JSON.parse(token);
    if (typeof found?.token === "string") return { "x-figma-token": found.token };
  } catch {
    return { authorization: `Bearer ${token}` };
  }
  return { authorization: `Bearer ${token}` };
}

function apiOf(ctx: Ctx, creds: Creds): Api {
  return { ctx, headers: headersOf(creds.token), base: base(ctx) };
}

function get(api: Api, path: string) {
  const answer = send(api.ctx, { method: "GET", url: `${api.base}${path}`, headers: api.headers });
  return json(checked(answer));
}

function ownHost(ctx: Ctx, url: string): boolean {
  const host = siteOf(url);
  const known = ctx.config.host;
  return host === known || host.endsWith(`.${known}`);
}

export function linkOf(ctx: Ctx, url: string): Link {
  const trimmed = url.trim();
  if (!ownHost(ctx, trimmed)) return fail("permanent", `${trimmed} is not a Figma link`);
  const path = trimmed.replace(/^https?:\/\/[^/]+/i, "").replace(/[?#].*$/, "").split("/").filter(Boolean);
  const at = path.findIndex((part) => KINDS.has(part));
  const branch = path.indexOf("branch");
  const file = branch > at && path[branch + 1] ? path[branch + 1] : path[at + 1];
  if (at < 0 || !file) return fail("permanent", `${trimmed} does not point to a Figma file`);
  const node = /[?&]node-id=([^&#]+)/.exec(trimmed)?.[1];
  const id = node ? decodeURIComponent(node).replace(/-/g, ":") : undefined;
  return id ? { file, node: id } : { file };
}

function walk(node, depth: number, lines: string[], texts: string[]) {
  if (!node || typeof node !== "object" || node.visible === false) return;
  if (node.type === "TEXT" && typeof node.characters === "string" && node.characters.trim() && texts.length < MOST_TEXTS) {
    texts.push(node.characters.trim());
  }
  if (depth <= DEEPEST && lines.length < MOST_LINES) {
    const shown = node.type === "TEXT" ? `: ${String(node.characters ?? "").trim().slice(0, 80)}` : "";
    lines.push(`${"  ".repeat(depth)}- ${node.name ?? ""} (${String(node.type ?? "").toLowerCase()})${shown}`);
  }
  for (const child of Array.isArray(node.children) ? node.children : []) walk(child, depth + 1, lines, texts);
}

function rendered(api: Api, link: Link): string | null {
  if (!link.node) return null;
  const found = get(api, `/v1/images/${link.file}?ids=${encodeURIComponent(link.node)}&format=png&scale=1`);
  const image = found?.images?.[link.node];
  return typeof image === "string" ? image : null;
}

function picture(api: Api, image: string | null) {
  if (!image) return null;
  let data = "";
  try {
    const answer = send(api.ctx, { method: "GET", url: image, binary: true });
    data = answer.status === 200 ? (answer.base64 ?? "") : "";
  } catch {
    return null;
  }
  if (data === "" || data.length > (MOST_PICTURE * 4) / 3) return null;
  return { media_type: "image/png", data };
}

export function frame(api: Api, url: string) {
  const link = linkOf(api.ctx, url);
  const whole = link.node ? get(api, `/v1/files/${link.file}/nodes?ids=${encodeURIComponent(link.node)}`) : get(api, `/v1/files/${link.file}?depth=3`);
  const root = link.node ? whole?.nodes?.[link.node]?.document : whole?.document;
  if (!root) return fail("permanent", `the Figma file ${link.file} has no node ${link.node ?? ""}; check the link`);
  const lines: string[] = [];
  const texts: string[] = [];
  walk(root, 0, lines, texts);
  const image = rendered(api, link);
  return {
    ref: link.node ? `${link.file}:${link.node}` : link.file,
    name: String(root.name ?? whole?.name ?? ""),
    file: String(whole?.name ?? ""),
    url,
    texts,
    outline: lines.join("\n"),
    image,
    picture: picture(api, image),
  };
}

export function call({ action, args, creds }: { action: string; args: Record<string, unknown>; creds: Creds }, ctx: Ctx) {
  if (action === "design.frame") return frame(apiOf(ctx, creds), text(args ?? {}, "url"));
  return unknownAction(action);
}

export function accounts({ creds }: { creds: Creds }, ctx: Ctx) {
  const api = apiOf(ctx, creds);
  if (heldBy(creds.token) !== null) {
    const user = userOf(get(api, "/api/session/state"));
    if (user === null) return fail("auth", "Figma has no account in this session; sign in to Figma in the browser first");
    return [{ id: `figma-${user.id}`, label: user.label, settings: {} }];
  }
  const me = get(api, "/v1/me");
  const id = textOf(me, ["id"]);
  if (id === "") return fail("auth", "Figma did not accept the token; create a personal access token with file_content:read");
  const label = textOf(me, ["handle"]) || textOf(me, ["email"]) || "Figma";
  return [{ id: `figma-${id}`, label, settings: {} }];
}
