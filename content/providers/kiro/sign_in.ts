import { type Tokens, callback, described, post, regional, unauthorized, words } from "sdk/providers/index.ts";
import { exchange, portal, redirect as socialRedirect, tokens as socialTokens } from "./auth.ts";
import { type Client, authorize, exchange as organizationExchange, firstProfile, portalAnswer, register, registered, tokens as organizationTokens } from "./idc.ts";

const PORTAL_CALLBACK = "/signin/callback";

function profile(ctx, accessToken: string): string {
  for (const region of words(ctx.config.profile_regions)) {
    const headers: [string, string][] = [
      ["authorization", `Bearer ${accessToken}`],
      ["content-type", "application/x-amz-json-1.0"],
      ["x-amz-target", "AmazonCodeWhispererService.ListAvailableProfiles"],
      ["user-agent", ctx.config.user_agent_runtime],
      ["x-amz-user-agent", ctx.config.user_agent_runtime],
    ];
    let reached;
    try {
      reached = ctx.http.post({ url: regional(ctx.config.q_url, region), headers, body: JSON.stringify({ maxResults: 10 }) });
    } catch (_) {
      continue;
    }
    const arn = firstProfile(reached.status, reached.body);
    if (arn !== undefined) return arn;
  }
  return unauthorized("signed in, but this account has no Kiro profile; ask your organization's administrator to add you to one");
}

function organization(ctx, client: Client, code: string, verifier: string, redirect: string): Tokens {
  const { status, body } = post(ctx, organizationExchange(ctx, client, code, verifier, redirect));
  const tokens = organizationTokens(ctx, client, { status, body, previous: null, now: ctx.clock.now() });
  return { ...tokens, account_id: profile(ctx, tokens.access_token) };
}

function social(ctx, answer: number, query: string, listened, verifier: string): Tokens {
  let code;
  try {
    code = ctx.callback.code({ answer });
    ctx.callback.finish({ answer, error: null });
  } catch (error) {
    ctx.callback.finish({ answer, error: described(error) });
    throw error;
  }
  const back = socialRedirect(ctx, listened.redirect, query);
  const { status, body } = post(ctx, exchange(ctx, code, verifier, back));
  return socialTokens(ctx, { status, body, previous: null, now: ctx.clock.now() });
}

function organizationSignIn(ctx, answer: number, listened, found: { start_url: string; region: string }): Tokens {
  const redirect = ctx.config.organization_redirect.replace("{port}", String(listened.port));
  let client: Client;
  try {
    const { status, body } = post(ctx, register(ctx, found.region, found.start_url, redirect));
    client = registered(found.region, status, body);
  } catch (error) {
    ctx.callback.finish({ answer, error: described(error) });
    throw error;
  }
  const verifier = ctx.crypto.random(64);
  const state = ctx.crypto.random(32);
  ctx.callback.redirect({ answer, to: authorize(ctx, client, redirect, state, ctx.oauth.challenge(verifier)) });
  const back = ctx.callback.next({ listener: listened.listener, state, others: [] }).answer;
  try {
    const tokens = organization(ctx, client, ctx.callback.code({ answer: back }), verifier, redirect);
    ctx.callback.finish({ answer: back, error: null });
    return tokens;
  } catch (error) {
    ctx.callback.finish({ answer: back, error: described(error) });
    throw error;
  }
}

export function signIn(ctx): Tokens {
  const [port, path] = callback(ctx.config.callback_port, ctx.config.callback_path);
  const listened = ctx.callback.listen({ port, path });
  const verifier = ctx.crypto.random(64);
  const state = ctx.crypto.random(32);
  ctx.browser.open({ url: portal(ctx, ctx.oauth.challenge(verifier), state, listened.redirect), code: null });
  const arrived = ctx.callback.next({ listener: listened.listener, state, others: [PORTAL_CALLBACK] });
  let found;
  try {
    found = portalAnswer(ctx, arrived.path, arrived.query);
  } catch (error) {
    ctx.callback.finish({ answer: arrived.answer, error: described(error) });
    throw error;
  }
  if ("social" in found) return social(ctx, arrived.answer, arrived.query, listened, verifier);
  return organizationSignIn(ctx, arrived.answer, listened, found);
}
