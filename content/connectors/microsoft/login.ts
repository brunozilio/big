import { words } from "sdk/connectors/index.ts";

type Config = { client_id: string; tenant: string; authorize_url: string; token_url: string; scopes: string };

type Ctx = { config: Config } & Record<string, any>;

const tenanted = (url: string, tenant: string) => url.replace("{tenant}", tenant && tenant !== "" ? tenant : "common");

const authorizeUrl = (ctx: Ctx) => tenanted(ctx.config.authorize_url, ctx.config.tenant);
const tokenUrl = (ctx: Ctx) => tenanted(ctx.config.token_url, ctx.config.tenant);

export const signIn = {
  name: (_: unknown, ctx: Ctx) => authorizeUrl(ctx),
  fields: () => [],
  begin: ({ port }: { port: number }, ctx: Ctx) =>
    ctx.oauth.code.begin({
      authorize_url: authorizeUrl(ctx),
      token_url: tokenUrl(ctx),
      client_id: ctx.config.client_id,
      scopes: words(ctx.config.scopes),
      port,
    }),
  exchange: ({ authorization, code, now }: { authorization: unknown; code: string; now: number }, ctx: Ctx) =>
    ctx.oauth.code.exchange({ token_url: tokenUrl(ctx), client_id: ctx.config.client_id, authorization, code, now }),
  refresh: ({ grant, now }: { grant: unknown; now: number }, ctx: Ctx) =>
    ctx.oauth.code.refresh({ token_url: tokenUrl(ctx), client_id: ctx.config.client_id, grant, now }),
};
