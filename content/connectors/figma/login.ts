import { checked, fail, json, send } from "sdk/connectors/index.ts";

type Ctx = { config: { host: string } } & Record<string, any>;

export type Session = { cookies: string; fuid: string };

const EXPIRES_IN = 900;
const INTERVAL = 2;
const SET_COOKIE = /,(?=\s*[A-Za-z0-9_.-]+=)|\n/;

export const base = (ctx: Ctx) => `https://www.${ctx.config.host}`;

function cookiesOf(headers: Record<string, string>): string {
  const key = Object.keys(headers).find((name) => name.toLowerCase() === "set-cookie");
  const raw = key === undefined ? "" : String(headers[key]);
  const pairs = raw.split(SET_COOKIE).map((cookie) => cookie.split(";")[0].trim()).filter((pair) => pair.includes("="));
  return pairs.join("; ");
}

export function heldBy(token: string): Session | null {
  try {
    const found = JSON.parse(token);
    return typeof found?.cookies === "string" && typeof found?.fuid === "string" ? found : null;
  } catch {
    return null;
  }
}

export function userOf(answer): { id: string; label: string } | null {
  const user = answer?.meta?.users?.[0];
  return typeof user?.id === "string" ? { id: user.id, label: String(user.handle || user.name || user.email || "Figma") } : null;
}

export function sessionOf(ctx: Ctx, cookies: string): Session {
  const headers = { accept: "application/json", "x-csrf-bypass": "yes", cookie: cookies };
  const user = userOf(json(checked(send(ctx, { method: "GET", url: `${base(ctx)}/api/session/state`, headers }))));
  if (user === null) fail("auth", "Figma has no account in this session; sign in to Figma in the browser first");
  json(checked(send(ctx, { method: "GET", url: `${base(ctx)}/api/user/state?fuid=${user.id}`, headers })));
  return { cookies, fuid: user.id };
}

export function grantOf(session: Session) {
  return { access_token: JSON.stringify(session), refresh_token: null, expires_at: null, scope: null, client: null, extra: null };
}

export const session = {
  name: (_, ctx: Ctx) => ctx.config.host,
  fields: () => [],
  start(_, ctx: Ctx) {
    const answer = send(ctx, {
      method: "POST",
      url: `${base(ctx)}/api/session/app_auth`,
      headers: { accept: "application/json", "content-type": "application/json", "x-csrf-bypass": "yes" },
      body: JSON.stringify({ app_type: "desktop" }),
    });
    const body = json(checked(answer));
    const id = typeof body?.meta?.id === "string" ? body.meta.id : "";
    const cookies = cookiesOf(answer.headers);
    if (id === "" || cookies === "") fail("auth", "Figma did not start the sign-in session");
    return {
      code: {
        device_code: id,
        user_code: "",
        verification_uri: `${base(ctx)}/login`,
        verification_uri_complete: null,
        expires_in: EXPIRES_IN,
        interval: INTERVAL,
      },
      carried: { cookies },
    };
  },
  poll({ started }, ctx: Ctx) {
    const cookies = String(started?.carried?.cookies ?? "");
    return { state: "authorized", grant: grantOf(sessionOf(ctx, cookies)) };
  },
  refresh({ grant }, ctx: Ctx) {
    const held = JSON.parse(String(grant?.access_token ?? "{}")) as Session;
    return grantOf(sessionOf(ctx, held.cookies));
  },
};
