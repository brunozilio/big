import { type Creds, type Language, checked, connect, fail, json, pick, send, siteOf, text, textOf, tokenField, unknownAction } from "sdk/connectors/index.ts";
import { design } from "sdk/connectors/contracts.ts";

const MOST_TEXTS = 200;
const MOST_LINES = 150;
const DEEPEST = 5;
const KINDS = new Set(["file", "design", "proto", "board", "slides", "buzz", "make"]);

type Ctx = { config: { api_url: string; hosts?: string } } & Record<string, any>;

type Api = { ctx: Ctx; token: string; base: string };

export type Link = { file: string; node?: string };

export function provider({ language }: { language?: Language }) {
  const help = pick(
    language ?? "pt-BR",
    "In Figma, open Settings › Security › Personal access tokens and create one with the file_content:read scope.",
    "En Figma, abre Settings › Security › Personal access tokens y crea uno con el alcance file_content:read.",
    "No Figma, abra Settings › Security › Personal access tokens e crie um token com o escopo file_content:read.",
  );
  return {
    name: "Figma",
    logo: "logo.svg",
    capabilities: ["design"],
    auth: { kind: "token", fields: [tokenField("token", pick(language ?? "pt-BR", "Personal access token", "Token de acceso personal", "Token de acesso pessoal"), help, true)], form: "bearer" },
    actions: design(),
  };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(language, "Prototypes and screens for the agents", "Prototipos y pantallas para los agentes", "Protótipos e telas para os agentes"),
    group: "design",
    button: connect(language),
    site: pick(language, "Figma", "de Figma", "do Figma"),
    choose_title: pick(language, "Figma account", "Cuenta de Figma", "Conta do Figma"),
    hidden: false,
  };
}

function apiOf(ctx: Ctx, creds: Creds): Api {
  return { ctx, token: creds.token, base: ctx.config.api_url.replace(/\/+$/, "") };
}

function get(api: Api, path: string) {
  const answer = send(api.ctx, { method: "GET", url: `${api.base}${path}`, headers: { "x-figma-token": api.token } });
  return json(checked(answer));
}

function ownHost(ctx: Ctx, url: string): boolean {
  const host = siteOf(url);
  const hosts = (ctx.config.hosts ?? "").split(/\s+/).filter(Boolean);
  return hosts.some((known) => host === known || host.endsWith(`.${known}`));
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

export function frame(api: Api, url: string) {
  const link = linkOf(api.ctx, url);
  const whole = link.node ? get(api, `/v1/files/${link.file}/nodes?ids=${encodeURIComponent(link.node)}`) : get(api, `/v1/files/${link.file}?depth=3`);
  const root = link.node ? whole?.nodes?.[link.node]?.document : whole?.document;
  if (!root) return fail("permanent", `the Figma file ${link.file} has no node ${link.node ?? ""}; check the link`);
  const lines: string[] = [];
  const texts: string[] = [];
  walk(root, 0, lines, texts);
  return {
    ref: link.node ? `${link.file}:${link.node}` : link.file,
    name: String(root.name ?? whole?.name ?? ""),
    file: String(whole?.name ?? ""),
    url,
    texts,
    outline: lines.join("\n"),
    image: rendered(api, link),
  };
}

export function call({ action, args, creds }: { action: string; args: Record<string, unknown>; creds: Creds }, ctx: Ctx) {
  if (action === "design.frame") return frame(apiOf(ctx, creds), text(args ?? {}, "url"));
  return unknownAction(action);
}

export function accounts({ creds }: { creds: Creds }, ctx: Ctx) {
  const me = get(apiOf(ctx, creds), "/v1/me");
  const id = textOf(me, ["id"]);
  if (id === "") return fail("auth", "Figma did not accept the token; create a personal access token with file_content:read");
  const label = textOf(me, ["handle"]) || textOf(me, ["email"]) || "Figma";
  return [{ id: `figma-${id}`, label, settings: {} }];
}
