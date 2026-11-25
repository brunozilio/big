---
name: datadog
description: Read before the first call to a Datadog tool (named mcp__datadog__*, or mcp__datadog-eu1__* and similar for other Datadog sites), which reach the Datadog account the user connected in Settings. Covers finding the right service and time window, searching and analyzing logs, metrics, traces and spans, monitors and incidents, keeping answers small, and what never to change.
---

# The Datadog tools

The `mcp__datadog__*` tools are Datadog's own MCP server, signed in with the
user's Datadog account. The server name follows the connection: `datadog` for
datadoghq.com, `datadog-eu1`, `datadog-us3` and so on for other sites. When no
such tool exists, Datadog is not connected; say so and point the user to
Settings → Connections instead of guessing.

The tools see only what the user's Datadog role sees. A permission error means
the account or the organization lacks `mcp_read` (or MCP Access is off); report
it, do not retry.

## Start narrow

1. **Name the service.** Find it from the code (`DD_SERVICE`, `service:` tags,
   the deploy manifest) or with `search_datadog_entities`. Every later query
   filters by `service:<name>` and, when known, `env:<env>`.
2. **Fix the window.** Use the time the user reported, the deploy time, or the
   last hour. Widen only when the window is empty.
3. **Ask for little.** Prefer aggregations and counts first, then a handful of
   raw events. Most tools take a `max_tokens` argument; keep it modest and page
   instead of pulling everything.

Read each tool's input schema before the first call; argument names differ
between tools.

## Which tool for what

| Question | Tools |
|---|---|
| What errors happened? | `analyze_datadog_logs` (counts by message, status, host), then `search_datadog_logs` for samples |
| Is it slow or failing per endpoint? | `aggregate_spans`, `search_datadog_spans` |
| What happened in one request? | `get_datadog_trace` with the trace id from a log or span |
| Did a number move? | `search_datadog_metrics` to find the name, `get_datadog_metric_context` for its tags, `get_datadog_metric` for the series |
| Is something alerting? | `search_datadog_monitors` (filter by service or status) |
| Is there an incident? | `search_datadog_incidents`, `get_datadog_incident` |
| What changed? | `search_datadog_events` (deploys, config changes) |
| Which hosts or entities? | `search_datadog_hosts`, `search_datadog_entities` |
| Front-end errors? | `search_datadog_rum_events`, `aggregate_rum_events` |

Other toolsets (APM health, error tracking, dashboards, notebooks) appear only
when the connection enables them; use what `ToolSearch` finds.

## Investigating a failure

1. Count errors for the service in the window, grouped by message.
2. Take the top message, pull two or three samples, and read their trace ids.
3. Open one trace and follow it to the failing span; note the resource,
   the error type and the downstream service.
4. Check monitors and events around the first error: a deploy or a config
   change right before it is the usual cause.
5. Map the finding back to the code with Grep and Read before proposing a fix.

Report what the data shows: the query, the window, the counts and one or two
sample ids or links, so the user can open them in Datadog. Mark a cause as
likely until the code or a trace confirms it.

## Never

- Write anything in Datadog (monitors, notebooks, incidents, mutes, workflow
  runs) unless the user asked for that exact change in this conversation.
- Paste secrets, tokens or personal data found in logs into the answer, files
  or commits. Summarize them instead.
- Run broad queries with no service, no window or no limit.
