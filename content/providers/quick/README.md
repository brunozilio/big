# Amazon Quick Subscription

Id `quick-subscription`. Amazon Quick is an agent that runs in AWS, not a
model API: it picks the model by mode (`fast`, `smart`, `advanced`) and runs
its own tools, so the turn carries the conversation as text and no tools.

## Sign-in

1. The browser opens Quick's discovery page (`social_url`, or
   `enterprise_url` when `discovery = "enterprise"` in `[quick]`) with
   `qwSurface=desktop:{nonce}:{port}:{version}`.
2. After the user signs in (AWS account, Google, Apple, GitHub, Amazon or the
   company SSO), Quick returns to `http://localhost:{port}/callback` with
   `nonce`, `account`, `issuerUrl` (IAM Identity Center), `tenantUrl` (the
   account's cell) and `idcRegion`.
3. The script registers a client at `oidc.{idcRegion}.amazonaws.com`
   (`client/register`, scope `quicksight:read_write`, redirect the same
   loopback), sends the browser to `/authorize` with PKCE and exchanges the
   code at `/token`. Refresh uses `refresh_token` on the same client.

The registered client and the tenant URL are kept in `Tokens.client`.

## Turns

The wire is `socket`: the turn opens
`wss://{tenant host}/streaming/quickwork-chat-bidirectional-stream?surfaceType=…&token=…&workingSessionId=…`,
sends `session_connect` and `user_message` as
`{"eventType":"rawInputEvent","payload":{"payload":base64(json)}}`, and reads
AWS event-stream frames whose payload carries the same base64 JSON:
`text_delta`, `thinking_delta`, `turn_usage`, `message_complete`, `error`.

## Usage and catalog

`POST {tenantUrl}/integration/quick-work/qbs-event` with an `invokeAgent`
envelope for `profile/usage` gives the session and monthly usage; the catalog
calls it to check the account and offers the modes from `[quick].modes`.

Verified end to end with a personal (social) account; the enterprise
discovery page is wired the same way but was not tried.
