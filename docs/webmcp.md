# WebMCP integration

WebMCP tools fit OAP without changing the profile format. OAP describes the maximum authority an
agent may request; WebMCP supplies a live, page-scoped inventory. A harness MUST intersect both
with its own policy before invocation.

Use stable capability names in portable profiles, such as `webmcp`, `webmcp/read`, and
`webmcp/call`. Do not place a page's transient JavaScript tool names in a reusable profile unless
the site publishes them as a stable contract. The harness maps a portable capability to the live
registry it discovered for the current page.

## Required host behavior

1. Configure exact HTTPS origins outside the portable profile. Origin allowlists are deployment
   policy and never travel as profile-granted authority.
2. Keep browser storage isolated by origin and never expose cookies, tokens, or storage values to
   the model.
3. Discover the live registry after navigation and bind invocation to a registry revision. If the
   page changes its registry, rediscover instead of invoking stale metadata.
4. Treat discovery as read-only information, not authorization. Apply the effective profile and
   host policy on every call.
5. Classify tools from trustworthy host policy plus WebMCP annotations. Missing annotations are
   not evidence that a tool is safe.
6. Record origin, URL, tool name, schema/revision digest, decision reference, result status, and
   timing in the audit trail. Redact credentials and sensitive arguments.

The example [`webmcp-researcher.agent.yaml`](../examples/webmcp-researcher.agent.yaml) requests
network access and portable WebMCP capabilities while leaving the origin choice to the harness.

## Portability

A harness that lacks WebMCP may narrow the capability away and report a degraded projection. A
broker may route a workload only to WebMCP-capable harnesses when the local binding marks it as
required. Neither behavior changes OAP's rule that profiles narrow authority and never widen it.
