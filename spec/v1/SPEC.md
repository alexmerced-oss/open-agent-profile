# Open Agent Profile Specification, Version 1.0

**Status:** Draft
**Spec version string:** `1.0`
**Schemas:** [`agent-profile.schema.json`](../../schema/v1/agent-profile.schema.json), [`agent-state-delta.schema.json`](../../schema/v1/agent-state-delta.schema.json)

## 1. Introduction

### 1.1 Purpose

Agent harnesses let users define named agents: a reviewer, a researcher, a data engineer, each with its own model, personality, tool access, and accumulated knowledge. Today that definition is either ephemeral (it lives in a chat session and dies with it) or proprietary (it lives in a format only one harness reads).

The Open Agent Profile format (OAP) defines a portable file that persists a named agent as **data rather than as a running process**. A conforming harness reads a profile to instantiate a fresh agent session on demand, and writes an updated revision back when that session ends. Nothing needs to stay resident between sessions. The profile is the agent's durable identity; a session is a temporary materialization of it.

### 1.2 Design goals

1. **A profile is data, not a program.** No executable code, no shell fragments, no secrets. A profile can be read, diffed, reviewed, and committed to version control safely.
2. **The controlling process owns persistence.** Agents propose changes to themselves; they do not write themselves. A session emits an `AgentStateDelta`, and the process that owns the file decides whether to apply it.
3. **Privilege narrows, never widens.** A profile expresses what an agent wants. The harness grants the intersection of that request and its own policy. Moving a profile to a new machine can never grant capability the harness would not otherwise give.
4. **Learned state is untrusted content.** Text an agent wrote about itself is injected as context, never as system authority.
5. **Portable by construction, extensible by namespace.** Every field maps onto concepts that real harnesses already have. Everything harness-specific goes in namespaced annotations.
6. **Degrade cleanly.** A harness that supports half the spec should still be able to run the agent, and should say plainly what it dropped.

### 1.3 Scope

In scope: the on-disk representation of a named agent, the rules for discovering and resolving it, the rules for instantiating a session from it, and the rules for updating it at session end.

Out of scope: the agent loop itself, wire protocols, tool implementations, model APIs, multi-agent orchestration topology, and any runtime IPC. OAP describes the file on disk and the contract around reading and writing it.

### 1.4 Terminology

The key words MUST, MUST NOT, REQUIRED, SHALL, SHOULD, SHOULD NOT, MAY, and OPTIONAL are to be interpreted as described in [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) and [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174).

| Term | Meaning |
| --- | --- |
| **Profile** | An OAP document of `kind: AgentProfile`. |
| **Harness** | Software that reads profiles and runs agents (Loro, MagAgent, Claude Code, and so on). |
| **Resolver** | The harness component that discovers, parses, validates, and materializes profiles. |
| **Session** | One instantiation of a profile: a running agent with a conversation. |
| **Delta** | An OAP document of `kind: AgentStateDelta` proposing updates to a profile. |
| **Applicator** | The component that validates a delta and persists the resulting revision. Normally the main process, not the agent. |
| **Resolved profile** | A profile after inheritance, variable substitution, and reference resolution, with no unresolved references left. |
| **Effective profile** | A resolved profile after the harness has intersected it with local policy. This is what actually runs. |

## 2. Document model

### 2.1 Encodings

A profile MAY be encoded as:

| Encoding | Extension | Notes |
| --- | --- | --- |
| YAML 1.2 | `.agent.yaml`, `.agent.yml` | RECOMMENDED for authoring. |
| JSON | `.agent.json` | Canonical form for digests and tooling. |
| Markdown with YAML frontmatter | `.agent.md` | Frontmatter is the document minus `spec.role.instructions`; the Markdown body is `spec.role.instructions`. |

All three encodings MUST produce identical logical documents. Conformance is defined against the JSON Schema; YAML and Markdown are transcodings of it. A resolver MUST accept at least one encoding and SHOULD accept all three.

Markdown encoding exists because several harnesses already store agent definitions that way, and because a long system prompt is far more pleasant to edit as a document body than as a YAML block scalar. In Markdown encoding, frontmatter MUST NOT contain `spec.role.instructions`; supplying both is an error.

### 2.2 Digest

The **profile digest** is `sha256:` followed by the hex SHA-256 of the canonical JSON encoding of the document, where canonical JSON means: object keys sorted lexicographically by Unicode code point, no insignificant whitespace, UTF-8, and no trailing newline. Digests are computed over the **whole document including `state` and `history`** unless a context explicitly says otherwise.

The **spec digest** is computed the same way over the two-key object `{"metadata": ..., "spec": ...}`. The spec digest is what pinning and trust decisions SHOULD use, because it does not change when an agent merely learns something.

### 2.3 Top-level structure

```yaml
oap: "1.0"            # REQUIRED
kind: AgentProfile    # REQUIRED
extends: []           # OPTIONAL, Level 3
metadata: {}          # REQUIRED: identity
spec: {}              # REQUIRED: the instantiation contract
state: {}             # OPTIONAL: what the agent has learned
history: []           # OPTIONAL: bounded revision log
```

The separation is load-bearing:

- `metadata` and `spec` are **authored by humans**. Agents may propose changes to them but MUST NOT apply changes to them.
- `state` is **written by sessions**, subject to `spec.lifecycle.writeback`.
- `history` is **append-only**, written by the applicator only.

An implementation that collapses this separation, for example by letting a session rewrite `spec.tools`, is not conforming. The boundary is the entire security model of writeback.

### 2.4 Versioning

`oap` carries `MAJOR.MINOR`. Within major version 1:

- A resolver MUST reject a document whose major version it does not implement.
- A resolver MUST accept a document with a higher minor version than it implements, ignoring fields it does not recognize, and SHOULD emit a warning naming them.
- New minor versions MUST NOT add required fields, remove fields, or change the meaning of existing fields.

`metadata.revision` is a separate, per-profile counter, unrelated to the spec version. It starts at 1 and increments by exactly 1 on every persisted write.

### 2.5 Unknown fields

The schema is `additionalProperties: false` throughout. Implementation-specific data goes in `metadata.annotations`, whose keys SHOULD be namespaced with a domain-like prefix:

```yaml
metadata:
  annotations:
    loro.io/tier: advanced
    magagent.dev/recipe: release-prep
```

Resolvers MUST preserve annotations they do not understand across a read/write cycle. Silently dropping another harness's annotations breaks round-tripping and is the most common way a portable format stops being portable.

## 3. Sections

This section is normative prose; the schema is normative structure. Where they disagree, the schema wins for structure and this document wins for behavior.

### 3.1 `metadata`

Identity that survives across sessions and machines.

| Field | Req | Notes |
| --- | --- | --- |
| `name` | yes | Lowercase slug, `^[a-z0-9](?:[a-z0-9._-]{0,62}[a-z0-9])?$`. Unique within a discovery root. |
| `description` | yes | One line stating **when to use this agent**. Harnesses use it for routing and selection, so "reviews Python for security bugs and missing tests" beats "a review agent". |
| `id` | no | Stable identifier surviving renames. URN or UUID. |
| `revision` | no | Defaults to 1. |
| `trust` | no | Assigned by the resolver from the discovery root. See §7.1. Any value present in the file MUST be discarded and recomputed at load. |
| `annotations` | no | Namespaced implementation data. |

The file name SHOULD be `<name>.agent.yaml`. When file name and `metadata.name` disagree, `metadata.name` wins and the resolver SHOULD warn.

### 3.2 `spec.role`

Who the agent is. This section becomes the system prompt.

`instructions` is REQUIRED and carries the substance. `objectives`, `constraints`, `persona`, `expertise`, and `examples` are structured supplements that a harness renders around it.

**Normative assembly order.** A conforming harness MUST assemble the system prompt in this order, so that the same profile produces comparable behavior across harnesses:

1. Harness preamble (harness-owned; safety rules, tool protocol, environment).
2. `spec.role.instructions`.
3. `spec.role.objectives`, if present.
4. `spec.role.persona`, if present.
5. `spec.role.constraints`, if present.
6. `spec.role.examples`, if present.
7. Profile state, per §3.9.
8. Harness postamble and the harness's own non-negotiable rules.

Steps 1 and 8 always take precedence over steps 2 through 7. A profile cannot override harness safety rules, and a harness MUST NOT allow it to. A harness MAY insert its own material between these steps but MUST NOT reorder them.

Structured persona fields exist so that a harness can render them in its own idiom, and so that tooling can diff "this agent got terser" without diffing a prose blob.

### 3.3 `spec.model`

Model selection, expressed at two levels of specificity so that profiles stay portable across providers:

- `provider` plus `id` name an exact model.
- `tier` (`minimal`, `standard`, `advanced`, `frontier`) is a portable capability hint.

Resolution order:

1. If `provider` and `id` are present and the harness can serve them, use them.
2. Otherwise, if `fallbacks` are present, try each in order.
3. Otherwise, if `tier` is present, map it through the harness's own tier routing.
4. Otherwise, use the harness default.

A harness MUST report which rule it applied when it did not use rule 1. Silently substituting a different model is the fastest way to make a profile's recorded behavior meaningless.

`parameters` are advisory. A harness MUST NOT fail because a provider does not support `seed` or `reasoning_effort`; it drops the unsupported parameter and warns.

### 3.4 `spec.tools`

What the agent may call.

`policy` selects the combination rule:

| `policy` | Effective tool set |
| --- | --- |
| `allowlist` (default) | `allow` ∩ harness-granted, then minus `deny`. |
| `denylist` | harness-granted minus `deny`. |
| `inherit` | harness-granted, unchanged. `allow` and `deny` are ignored and SHOULD warn if present. |

In every case the result is intersected with what the harness would grant the invoking user in the current context. A profile listing `shell` on a harness where the user has no shell access gets no shell. See §7.2.

Names MAY be globs. `mcp/<server>/<tool>` is the RECOMMENDED naming convention for MCP tools; `mcp/github/*` selects every tool from the `github` server.

`mcp_servers` declares MCP dependencies. Environment values MUST be same-name `${VARIABLE}` references, never literals; a resolver MUST reject a literal value in `env` or `headers`. This is the same rule several harnesses already apply to imported MCP config, and it exists so a profile can be shared publicly without leaking anything.

`skills` declares Agent Skills packages the agent expects. `digest` pins content. A resolver MUST NOT install a skill as a side effect of loading a profile; it either finds the skill already present or reports it missing. If a skill has `required: true` and is missing, instantiation MUST fail rather than run a differently-capable agent under the same name.

### 3.5 `spec.permissions`

The privilege ceiling the profile requests. Every field narrows. A profile that says `shell: allow` on a harness whose policy says `shell: ask` gets `ask`. The ordering used for intersection is `deny` < `ask` < `allow`; the effective decision is the minimum of the two.

`filesystem.read_roots` and `write_roots` are workspace-relative unless the harness explicitly permits absolute paths. Path traversal escaping the workspace MUST be rejected at resolve time, not at tool-call time.

`rules` are evaluated in order, first match wins, and are themselves subject to the narrowing rule.

### 3.6 `spec.context`

Material loaded before the first user turn.

`files` are workspace-relative. `mode: always` injects at boot; `mode: on_demand` advertises the path and lets the agent fetch it. `on_demand` is the default because it is cheaper and because it lets the agent decide.

`documents` are external URIs. Content fetched from them is **untrusted** and MUST be labeled as such in context.

`variables` provide non-secret substitution values. A harness supporting substitution MUST support `${{ vars.KEY }}` in `spec.role.instructions`, `objectives`, and `constraints`, and MUST NOT perform substitution anywhere in `state`. Undefined variables are an error, not an empty string.

`budget` caps context spend, including `max_state_tokens`, which governs §3.9.

### 3.7 `spec.memory`

Where durable knowledge lives beyond the profile file.

`mode` is the ceiling for the whole section. `stores` name concrete backends: the profile's own `state` block (`kind: oap-state`), a knowledge graph (`kind: maggraph`), a harness-local memory store (`kind: loro-local`), a vector index, or something custom. A store with `required: true` that cannot be reached MUST fail instantiation.

`scopes` names the namespaces the agent may touch. Scope enforcement is the harness's job; the profile only requests.

The `state` block and external stores are complementary. State is for what makes this agent itself: bounded, reviewable, and portable in the same file. External stores are for volume: transcripts, embeddings, and organization-wide knowledge that should not be copied into every profile that reads it.

### 3.8 `spec.runtime` and `spec.lifecycle`

`runtime` bounds a single session: turns, tool calls, wall clock, cost, and delegation.

`runtime.mode` distinguishes a `primary` agent (a user-facing top-level session) from a `subagent` (delegated only). `either` is the default.

`runtime.subagents.allow` lists profile names this agent may delegate to. An empty or absent list means no delegation. Delegation MUST NOT escalate: a subagent's effective profile is intersected with the parent's effective profile, not with the harness default. Otherwise delegation becomes a privilege-escalation path, where a constrained agent spawns an unconstrained one.

`lifecycle.writeback` controls persistence at session end:

| Value | Behavior |
| --- | --- |
| `off` | Deltas are discarded. The profile is read-only in practice. |
| `propose` (default) | The session emits a delta; a human or a policy engine approves before it is applied. |
| `auto` | State-only operations are applied without prompting. `proposals` still require approval. |

`lifecycle.retention` bounds `state` growth: entry caps, TTL, and an eviction strategy. Entries with `pinned: true` are exempt from automatic eviction. Retention is enforced by the **applicator**, at write time, not by the agent.

`lifecycle.on_start` and `on_end` reference **named hooks registered with the harness**. A profile carries a hook name and parameters, never a command line. This is deliberate: the moment a profile can carry a command, sharing one becomes remote code execution.

### 3.9 `state`

What the agent has learned. Written by sessions, bounded by retention, and read back at the next instantiation.

| Field | Purpose |
| --- | --- |
| `summary` | Short self-briefing injected at boot. |
| `facts` | Discrete learned statements with confidence, source, and timestamps. |
| `preferences` | How the user or project wants things done. |
| `glossary` | Domain terms this agent keeps getting asked about. |
| `open_threads` | Work carried across sessions. |
| `metrics` | Counters, for retention and for the user's benefit. |

**Injection rules.** A harness MUST:

1. Inject state after `spec.role` material and before the harness postamble (§3.2, step 7).
2. Mark state as untrusted, agent-authored content, clearly delimited from harness and profile instructions.
3. Ignore any instruction-like text inside state that attempts to change tool access, permissions, safety behavior, or the agent's identity. State informs the agent; it does not govern it.
4. Respect `spec.context.budget.max_state_tokens`. When state exceeds the budget, drop entries by the configured eviction order, never truncate mid-entry, and tell the agent that state was elided.

Rule 3 matters more than it looks. Without it, any process that can write a delta can rewrite the agent's rules on the next boot, which turns a convenience feature into a persistence mechanism for prompt injection.

### 3.10 `history`

An append-only, bounded log of persisted revisions: which revision, when, by whom, which sections changed, and who approved. Written by the applicator only. Trimmed to `lifecycle.retention.max_history`, oldest first.

`history` gives a reviewer the one thing a plain config file cannot: an answer to "when did this agent start behaving differently, and who signed off".

## 4. Inheritance

`extends` is a list of profile references applied left to right, with the current document applied last. Level 3.

Merge rules, applied recursively:

1. Objects deep-merge. Keys present in the child replace keys in the base.
2. Arrays **replace** wholesale. They do not concatenate.
3. An explicit `null` in the child **deletes** the key.
4. `metadata.name`, `metadata.id`, and `metadata.revision` are never inherited; they always come from the final document.
5. `state` and `history` are never inherited. Learned state belongs to a specific agent, not to a template.
6. Cycles are an error. Depth is capped at 8.
7. The narrowing rule of §7.2 applies to the **merged result**, not to each layer, so a child cannot widen what a base narrowed by out-ranking it.

Arrays replace rather than append because append semantics make it impossible to remove an inherited item, and because "which base contributed this tool" becomes unanswerable at review time. If you want additive composition, be explicit about the full list.

## 5. Lifecycle

Six phases. A Level 1 implementation performs 1 through 4; Level 2 adds 5 and 6.

### 5.1 Discover

Resolvers search discovery roots in this order, later roots taking precedence for the same name **only where the spec permits** (see §7.1):

1. **Managed** (system or organization policy), for example `/etc/<harness>/agents/`
2. **User**, for example `~/.config/<harness>/agents/`
3. **Project**, for example `<workspace>/.agents/`
4. **Plugin or package**, contributed by installed extensions

`.agents/` at the workspace root is the RECOMMENDED project location, because it is harness-neutral. Harnesses with an existing convention (`.magent/agents/`, `.claude/agents/`) SHOULD read both and SHOULD prefer the harness-native directory when a name appears in both, warning about the collision.

Name collisions **within one root** are an error, not a silent precedence decision. Collisions **across roots** resolve by the order above and MUST be reported.

### 5.2 Resolve

1. Parse per encoding.
2. Validate against the JSON Schema. Reject on failure.
3. Apply `extends` (§4).
4. Assign `metadata.trust` from the discovery root, discarding any value in the file.
5. Substitute `${{ vars.KEY }}` in role text. Undefined variable is an error.
6. Resolve `${ENV}` references in `mcp_servers`. A missing variable for a `required` server is an error; otherwise the server is dropped with a warning.
7. Verify digests where pinned.
8. Reject paths escaping the workspace.

The output is a **resolved profile**. Resolution MUST be free of side effects: no installs, no network writes, no file creation.

### 5.3 Instantiate

The harness intersects the resolved profile with local policy (§7.2) to produce the **effective profile**, then builds the session: system prompt per §3.2, tool set per §3.4, permissions per §3.5, context per §3.6, state per §3.9.

The harness MUST record, and SHOULD be able to show the user on request:

- The profile name, revision, and spec digest used.
- Every field it dropped, narrowed, or substituted, with the reason.

That second record is what makes portability honest. A profile that quietly runs with half its tools missing looks like a working agent right up until it does not.

### 5.4 Run

The resolved profile is **immutable for the duration of the session**. A harness MUST NOT reload a profile mid-session in a way that changes the effective contract. If the file changes on disk during a session, the change applies at the next instantiation.

The session MAY accumulate a pending delta as it goes.

### 5.5 Reconcile

At session end the session produces an `AgentStateDelta` (§6). Producing a delta MUST NOT write to the profile.

A harness SHOULD generate delta operations from concrete evidence in the session rather than by asking the model to freely rewrite state. Recommended sources: explicit user statements ("always use pytest, not unittest"), decisions recorded during the session, and thread status changes. Speculative inference produces state that drifts.

### 5.6 Persist

The applicator:

1. Validates the delta against the delta schema.
2. Checks `target.revision` against the profile's current revision. A mismatch is a **conflict**; see §6.3.
3. Verifies `target.digest` if present.
4. Rejects any operation whose path is outside `/state`.
5. Applies `spec.lifecycle.writeback`: discard, queue for approval, or apply.
6. Applies operations atomically. All or nothing.
7. Enforces `lifecycle.retention`.
8. Increments `metadata.revision` by 1 and sets `metadata.updated_at` and `state.updated_at`.
9. Appends a `history` entry and trims history.
10. Writes the file atomically (write to a temporary file in the same directory, `fsync`, then rename).
11. Routes every entry in `proposals` to human approval. Never applies them automatically.

Step 6 plus step 10 together mean a crashed or killed process never leaves a half-updated agent on disk. Given that this file is the agent's identity, that property is worth the small cost.

## 6. State deltas

### 6.1 Shape

```yaml
oap: "1.0"
kind: AgentStateDelta
target:
  name: code-reviewer
  revision: 7
  digest: sha256:...
session:
  id: sess_01J...
  harness: loro
  ended_at: 2026-08-14T18:04:11Z
  outcome: completed
summary: Learned the team's test-layout convention and closed the flaky-test thread.
operations:
  - op: add
    path: /state/facts/-
    value:
      id: fact-test-layout
      text: Tests live beside the module as test_<module>.py, not in a top-level tests/ tree.
      confidence: 0.9
      source: user statement, 2026-08-14
      learned_at: 2026-08-14T17:52:00Z
    reason: User corrected the agent twice on file placement.
proposals:
  - path: /spec/tools/allow
    op: replace
    value: [read, search, edit, shell]
    rationale: The agent could not run the test suite it was asked to fix.
```

### 6.2 Operation rules

- `op` is `add`, `replace`, or `remove`. `move`, `copy`, and `test` are reserved.
- `path` is an RFC 6901 JSON Pointer rooted at `/state`. `-` appends to an array.
- Entries in `facts`, `preferences`, and `open_threads` are addressed by `id`. An applicator MUST support pointer paths and SHOULD additionally accept the convenience form `/state/facts/id:<entry-id>`, resolving it to the current index at apply time.
- `add` to a path that already exists is a `replace` for objects and an insert for arrays.
- `remove` on a missing path is a no-op with a warning, not an error, because sessions run concurrently and idempotency is worth more than strictness here.

### 6.3 Conflicts

If `target.revision` does not match the profile's current revision, another writer got there first. The applicator MUST NOT blind-write. It MUST do one of:

1. **Rebase** the delta onto the current revision when every operation still applies cleanly. Operations addressing entries by `id` usually do; operations addressing array indices usually do not.
2. **Queue** the delta for human resolution.
3. **Reject** the delta, reporting both revisions.

Concurrent sessions from the same profile are expected and normal. This is why entries carry stable `id`s.

### 6.4 The proposals boundary

`operations` are mechanical. `proposals` are political.

A session cannot grant itself a tool, widen a permission, change its model, or edit its own instructions. It can only ask, in writing, with a rationale, and a human decides. An applicator MUST NOT apply a proposal automatically under any configuration, including `writeback: auto`. An applicator MUST classify any proposal that widens `spec.tools`, `spec.permissions`, `spec.memory`, or `spec.runtime.subagents` as high risk regardless of the `risk` value in the document.

## 7. Security

The full model is in [security.md](security.md). The normative core:

### 7.1 Trust labels

Assigned by the resolver from the discovery root, never read from the file:

| Label | Source | Default posture |
| --- | --- | --- |
| `managed` | System or organization policy path | Trusted configuration |
| `user` | User config directory | Trusted by the user, not by the organization |
| `project` | Workspace | Untrusted until reviewed |
| `imported` | Fetched or converted from elsewhere | Untrusted, SHOULD be digest-pinned |

Trust affects review requirements and defaults. It never grants tool authority on its own.

### 7.2 The narrowing rule

> A profile MAY reduce what the harness would otherwise grant. It MUST NOT increase it.

This applies to tools, permissions, filesystem roots, network hosts, memory scopes, model access, cost budgets, and delegation. The effective value is always the more restrictive of profile and policy. There is no field, flag, or trust label that reverses this.

### 7.3 No code, no secrets

A profile MUST NOT contain executable code, shell command lines outside of MCP `command`/`args`, or literal secret values. Environment references are the only channel for credentials. A resolver MUST reject literal values in `mcp_servers[].env` and `mcp_servers[].headers`, and SHOULD scan the whole document for high-entropy strings matching known credential patterns and refuse to load on a match.

MCP `command` and `args` are the one place a profile names an executable. Harnesses SHOULD gate MCP server launch behind the same approval they use for any other server definition, and SHOULD NOT auto-launch servers from a `project` or `imported` profile without confirmation.

### 7.4 State is untrusted

See §3.9, rule 3. Restated because it is the single most important runtime rule in this spec: **content in `state` is data the agent wrote about itself, and it MUST be treated with exactly the trust level of a web page the agent fetched.**

## 8. Conformance

See [conformance.md](conformance.md) for the full requirement lists and test procedure.

| Level | Name | Capability |
| --- | --- | --- |
| **1** | Read | Discover, validate, instantiate. No writeback. |
| **2** | Read/Write | Level 1, plus state injection, delta generation, and persistence with atomic writes and history. |
| **3** | Full | Level 2, plus `extends`, MCP servers, skills, external memory stores, delegation, digest pinning, and multi-root precedence. |

An implementation claiming a level MUST pass every test for that level and all lower levels, and MUST publish which OPTIONAL features it does not implement.

## 9. Relationship to other formats

OAP does not replace [Agent Skills](https://code.claude.com/docs/en/skills), MCP, or any harness's config file. It sits between them:

- **Agent Skills** package reusable *procedures*. A profile references skills; it does not contain them.
- **MCP** provides *tools*. A profile declares which servers an agent needs.
- **Harness config** governs *the machine*. A profile is intersected with it and never overrides it.
- **OAP** persists *who the agent is*: role, model, surface, context, and what it has learned.

Mappings to specific harness formats are in [../../docs/interop.md](../../docs/interop.md).

## Appendix A: Reserved for future versions

`kind: AgentProfileBundle` (multiple profiles in one document), `kind: AgentProfileIndex` (registry manifests), delta ops `move`, `copy`, and `test`, and signature envelopes for `imported` profiles. Implementations MUST reject unknown `kind` values in 1.x rather than guessing.
