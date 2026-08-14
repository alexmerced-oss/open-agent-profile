# Field reference

Every field in OAP v1, what it does, and what it defaults to. The normative structure is [`agent-profile.schema.json`](../schema/v1/agent-profile.schema.json); this is the readable version.

Required fields are marked **R**.

## Top level

| Field | | Default | Notes |
| --- | --- | --- | --- |
| `oap` | **R** | | `"1.0"`. Major version gates acceptance. |
| `kind` | **R** | | `AgentProfile`. Unknown kinds are rejected. |
| `extends` | | none | Base profiles, applied left to right. Level 3. |
| `metadata` | **R** | | Identity. |
| `spec` | **R** | | The instantiation contract. Human-authored. |
| `state` | | none | Learned, session-written. |
| `history` | | none | Append-only revision log, applicator-written. |

## `metadata`

| Field | | Default | Notes |
| --- | --- | --- | --- |
| `name` | **R** | | Lowercase slug. Unique within a discovery root. |
| `description` | **R** | | One line saying **when to use this agent**. Harnesses route on it. |
| `id` | | none | Stable identifier surviving renames. URN or UUID. |
| `display_name` | | `name` | Human-facing label. |
| `revision` | | `1` | Increments by exactly 1 per persisted write. |
| `created_at`, `updated_at` | | none | RFC 3339. Quote them in YAML, see [implementers guide](implementers-guide.md#yaml-timestamps). |
| `authors`, `tags`, `license`, `homepage` | | none | Provenance. |
| `trust` | | computed | Set by the resolver from the discovery root. A value in the file is discarded. |
| `annotations` | | none | Namespaced implementation data. Round-trips by contract. |

Write `description` for a router, not for a README. "Reviews Python for security bugs and missing tests" is useful; "a review agent" is not.

## `spec.role`

The system prompt.

| Field | | Notes |
| --- | --- | --- |
| `instructions` | **R** | The substance. Markdown. In `.agent.md` encoding this is the document body. |
| `objectives` | | What the agent is trying to achieve. |
| `constraints` | | Hard limits. Rendered **after** instructions, so they win on conflict. |
| `persona.tone` | | Free text. |
| `persona.voice` | | Free text. |
| `persona.verbosity` | | `terse` \| `balanced` \| `detailed`. |
| `persona.formatting` | | How output should be shaped. |
| `persona.language` | | BCP 47 tag. |
| `persona.style_rules` | | Specific do/don't rules. |
| `expertise` | | Domains. Some harnesses use this for routing. |
| `examples` | | Few-shot `{input, output, note}` triples. |

Assembly order is normative: harness preamble, instructions, objectives, persona, constraints, examples, state, harness postamble. Put anything that must not be overridden into `constraints`.

## `spec.model`

| Field | Default | Notes |
| --- | --- | --- |
| `provider`, `id` | harness default | Exact model. |
| `tier` | none | `minimal` \| `standard` \| `advanced` \| `frontier`. Portable fallback when the exact model is unavailable. |
| `fallbacks` | none | Ordered `{provider, id}` alternates. |
| `parameters.temperature` | | 0 to 2. |
| `parameters.top_p` | | 0 to 1. |
| `parameters.max_output_tokens` | | |
| `parameters.reasoning_effort` | | `none` \| `low` \| `medium` \| `high` \| `max`. |
| `parameters.stop`, `parameters.seed` | | Advisory. Unsupported parameters are dropped with a warning, never an error. |

Resolution order: exact model, then fallbacks, then tier, then harness default. A harness must report which rule it used when it was not the first.

## `spec.tools`

| Field | Default | Notes |
| --- | --- | --- |
| `policy` | `allowlist` | `allowlist` \| `denylist` \| `inherit`. |
| `allow` | none | Names or globs. With `allowlist` and an empty list, the agent gets no tools. |
| `deny` | none | Applied after `allow`. |
| `bindings[].name` | | Tool to configure. A binding for an ungranted tool is inert. |
| `bindings[].permission` | | `allow` \| `ask` \| `deny`, narrowing only. |
| `bindings[].config` | | Non-secret, implementation-defined. |
| `mcp_servers` | none | See below. Level 3. |
| `skills` | none | `{name, source, digest, required}`. Referenced, never installed. Level 3. |

`mcp/<server>/<tool>` is the recommended tool naming convention, so `mcp/github/*` selects a whole server.

### `spec.tools.mcp_servers[]`

| Field | Notes |
| --- | --- |
| `name` | **R** |
| `transport` | **R**. `stdio` or `http`. |
| `command`, `args` | Required for `stdio`. |
| `url` | Required for `http`. |
| `env` | Values MUST be `${VARIABLE}`. Literals are rejected. |
| `headers` | Values MUST be `${VARIABLE}` or `Bearer ${VARIABLE}`. |
| `tools.allow`, `tools.deny` | Per-server tool filtering. |

## `spec.permissions`

Every field here narrows. It never widens.

| Field | Notes |
| --- | --- |
| `default`, `shell`, `edit`, `network` | `allow` \| `ask` \| `deny`. Effective value is the minimum of profile and policy on `deny < ask < allow`. |
| `filesystem.read_roots` | Workspace-relative. |
| `filesystem.write_roots` | Workspace-relative. |
| `filesystem.deny_paths` | Applied last. |
| `allow_hosts` | Outbound host allowlist. |
| `rules[]` | `{tool, action, target, decision, reason}`, first match wins. |

## `spec.context`

| Field | Default | Notes |
| --- | --- | --- |
| `working_directory` | `.` | Workspace-relative. |
| `files[].path` | | Workspace-relative. Traversal is rejected at resolve time. |
| `files[].mode` | `on_demand` | `always` injects at boot; `on_demand` advertises the path. |
| `files[].max_bytes` | | Cap per file. |
| `documents[].uri` | | External. Content is untrusted. |
| `documents[].digest` | | Pin. |
| `variables` | | Non-secret `${{ vars.KEY }}` substitutions for role text. Undefined is an error. |
| `budget.max_context_tokens` | | Total context ceiling. |
| `budget.max_state_tokens` | | Ceiling on injected state. Excess drops whole entries. |
| `budget.max_state_bytes` | | Same, in bytes. |

Prefer `on_demand`. It is cheaper and it lets the agent decide what it needs.

## `spec.memory`

| Field | Default | Notes |
| --- | --- | --- |
| `mode` | `read_only` | `off` \| `read_only` \| `read_write`. Ceiling for the section. |
| `scopes` | none | Namespaces the agent may touch. |
| `stores[].name` | | Label. |
| `stores[].kind` | | `oap-state`, `maggraph`, `loro-local`, `vector`, `custom`, and so on. |
| `stores[].uri` | | Location. |
| `stores[].mode` | `read_only` | Per-store ceiling. |
| `stores[].required` | `false` | When true, an unreachable store fails instantiation. |

## `spec.runtime`

| Field | Default | Notes |
| --- | --- | --- |
| `mode` | `either` | `primary` \| `subagent` \| `either`. |
| `max_turns`, `max_tool_calls`, `timeout_seconds`, `max_cost_usd` | harness default | Narrowing only. |
| `subagents.allow` | none | Profile names this agent may delegate to. Absent means no delegation. |
| `subagents.max_concurrent`, `max_depth` | | Bounds. |

A subagent's effective profile is intersected with the **parent's**, not the harness default. Delegation cannot escalate.

## `spec.lifecycle`

| Field | Default | Notes |
| --- | --- | --- |
| `writeback` | `propose` | `off` \| `propose` \| `auto`. |
| `retention.max_facts` | none | Cap. Pinned entries are exempt. |
| `retention.max_open_threads` | none | Cap. Closed threads evict first. |
| `retention.fact_ttl_days` | none | Age-out. |
| `retention.eviction` | `least_recently_used` | `oldest` \| `least_confident` \| `least_recently_used`. |
| `retention.max_history` | `50` | History trim, oldest first. |
| `on_start[]`, `on_end[]` | none | `{hook, required, with}`. Named hooks registered with the harness. **Never command lines.** |

## `state`

Session-written. Every string here is untrusted content.

| Field | Notes |
| --- | --- |
| `revision` | State-only counter, separate from `metadata.revision`. |
| `updated_at` | Set by the applicator. |
| `summary` | Short self-briefing injected at boot. |
| `facts[]` | Learned statements. |
| `preferences[]` | How the user or project wants things done. |
| `glossary[]` | `{term, definition, source}`. |
| `open_threads[]` | `{id, title, detail, status, opened_at, updated_at, refs}`. Work carried across sessions. |
| `metrics` | `{sessions, last_session_at, total_tool_calls, total_cost_usd}`. |

### Entry shape, for `facts` and `preferences`

| Field | | Notes |
| --- | --- | --- |
| `id` | **R** | Stable. Deltas address entries by it, which is what makes concurrent sessions rebaseable. |
| `text` | **R** | The statement. |
| `confidence` | | 0 to 1. |
| `source` | | Where it came from. Surface this in review UIs; a user statement and a fetched web page deserve different scrutiny. |
| `learned_at`, `last_used_at`, `expires_at` | | RFC 3339. |
| `scope` | | Memory namespace. |
| `tags` | | Free-form. |
| `pinned` | | Exempt from automatic eviction. |

## `history[]`

Applicator-written, append-only, trimmed to `max_history`.

| Field | | Notes |
| --- | --- | --- |
| `revision` | **R** | |
| `at` | **R** | |
| `by` | | Session id, harness, or human identity. |
| `session_id`, `harness` | | Provenance. |
| `change` | | Human-readable summary. |
| `approved_by` | | Who signed off. |
| `sections` | | Which of `metadata`, `spec`, `state` changed. |
