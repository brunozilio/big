# Amazon Quick

Connector for the `assistant` capability: `assistant.ask` puts a question to
Amazon Quick, which answers from the company's email, chats, documents, data
and knowledge, and returns `{ answer, sources, thread }`. Passing `thread`
back continues the same conversation; `mode` picks `fast`, `smart` or
`advanced`.

## Sign-in (loopback in two hops, port `[quick].loopback_port`)

1. `begin` opens Quick's discovery page (`social_url`, or `enterprise_url`
   when `discovery = "enterprise"`) with
   `qwSurface=desktop:{nonce}:{port}:{version}`.
2. Quick returns to `http://localhost:{port}/callback` with `nonce`,
   `account`, `issuerUrl` (IAM Identity Center), `tenantUrl` (the account's
   cell) and `idcRegion`. There is no code yet, so `proceed` registers a
   public client at `oidc.{idcRegion}.amazonaws.com/client/register` (the
   redirect must be the bare loopback `http://127.0.0.1:{port}`) and sends the
   browser to `/authorize` with PKCE.
3. The code comes back to the bare loopback and `exchange` trades it at
   `/token`. The access token and the tenant ride in the grant's token; the
   registered client stays in `extra` for `refresh`.

## Asking

`ctx.socket` opens
`wss://{tenant host}/streaming/quickwork-chat-bidirectional-stream?surfaceType=…&token=…&workingSessionId=…`,
sends `session_connect` and `user_message` as
`{"eventType":"rawInputEvent","payload":{"payload":base64(json)}}`, and reads
AWS event-stream frames until `message_complete`. Tools Quick asks the client
to run (`tool_call`) are refused, since this client runs none. A
`<sources>` block of markdown links in the answer becomes `sources`.

The protocol is Quick's own and undocumented; `app_version` and
`surface_type` copy the desktop app.
