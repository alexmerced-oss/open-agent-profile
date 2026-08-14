# Interop

OAP is designed to map onto formats harnesses already have, not to replace them. This document gives the field mappings and the honest gaps.

## Where OAP sits

| Standard | Persists | Relationship |
| --- | --- | --- |
| [Agent Skills](https://code.claude.com/docs/en/skills) | Reusable procedures | A profile **references** skills. It never contains them. |
| [MCP](https://modelcontextprotocol.io) | Tool servers | A profile **declares** which servers an agent needs. |
| Harness config | Machine policy | A profile is **intersected with** it, never overrides it. |
| **OAP** | **Who the agent is, and what it learned** | The gap the others leave. |

A useful way to hold it: Skills are what an agent knows how to do, MCP is what it can reach, harness config is what it is allowed to do, and OAP is who it is.

## MagAgent (`.magent/agents/*.md`)

MagAgent already stores agent definitions as Markdown with YAML frontmatter, which is the closest existing format to OAP's Markdown encoding.

| MagAgent | OAP | Notes |
| --- | --- | --- |
| file body | `spec.role.instructions` | Same convention. |
| `name` | `metadata.name` | |
| `description` | `metadata.description` | |
| `mode` | `spec.runtime.mode` | `subagent` / `primary`. |
| `provider` | `spec.model.provider` | |
| `model` | `spec.model.id` | |
| `tools` (map) | `spec.tools.bindings[].permission` | `{write: false}` becomes a binding with `permission: deny`; `{shell: "ask"}` becomes `permission: ask`. |
| `permissionMode` | `spec.permissions.default` | Named modes map onto decisions. |
| `memory` | `spec.memory.mode` | |
| `maxTurns` | `spec.runtime.max_turns` | |
| built-in agents (`@review`, `@explore`, `@docs`) | profiles shipped in a managed root | Keeps `@name` invocation working. |

**Gap:** MagAgent has no `state`, no `history`, no writeback, and no delta concept. That is the part OAP adds. Its MagGraph memory becomes a `spec.memory.stores` entry of `kind: maggraph`, which means an agent's durable knowledge is split deliberately: bounded identity in the profile, volume in the graph.

## Loro

Loro has no agent-definition format today. It has the surrounding machinery, which is most of the work: a permission engine, an approvals manager, an audit log, a sandbox, an identity context, a skills registry with trust labels and digest pinning, and MCP support.

| Loro concept | OAP |
| --- | --- |
| `PermissionsConfig` | `spec.permissions`, intersected |
| `PermissionRuleConfig` | `spec.permissions.rules[]` |
| `ModelConfig`, `ModelTierConfig` | `spec.model`, with `tier` mapping directly onto Loro's `minimal`/`standard`/`advanced`/`frontier` |
| skills trust labels (`enterprise-managed`, `untrusted-local`) | `metadata.trust` (`managed`, `user`, `project`, `imported`) |
| skills digest pinning | `extends[].digest`, `spec.tools.skills[].digest`, spec digest |
| `ApprovalManager` | the `proposals` gate |
| audit log | `history` entries, plus Loro's own audit events |
| `LocalMemoryConfig`, `SharedMemoryConfig` | `spec.memory.stores` of kind `loro-local` and `loro-shared` |

OAP's `tier` field exists in the format largely because Loro's tier routing showed that portable model selection needs a capability level, not just a model id.

**Gap:** everything. Loro needs the profile layer built. See the implementation plan in the Loro repository.

## Claude Code (`.claude/agents/*.md`)

Claude Code stores subagents as Markdown with frontmatter. Check your installed version for the exact field set; the mapping below covers the common fields.

| Claude Code | OAP |
| --- | --- |
| file body | `spec.role.instructions` |
| `name` | `metadata.name` |
| `description` | `metadata.description` |
| `tools` | `spec.tools.allow` |
| `model` | `spec.model.id` or `spec.model.tier` |

**Gap:** no persistence layer. This is the case the bundled [skills](../skills/) exist for: they let Claude Code read a profile, run as it, and emit a delta at session end without native support.

## OpenAI Assistants-style APIs

| Assistants | OAP |
| --- | --- |
| `name` | `metadata.name` |
| `instructions` | `spec.role.instructions` |
| `model` | `spec.model.id` |
| `tools` | `spec.tools.allow` |
| `metadata` | `metadata.annotations` |
| file attachments | `spec.context.files` / `documents` |

**Gap:** the agent is server-side state rather than a file, so there is no local revision to diff or review. A profile can be projected onto an assistant at creation time; the profile stays the source of truth and the assistant is a cache.

## Converting to OAP

A converter is a Level 1 concern and mostly mechanical. Four rules keep it honest:

1. **Never invent narrowing you cannot verify.** If the source format has no permission concept, emit no `spec.permissions` rather than guessing at something restrictive-looking. A permissions block nobody wrote is worse than none, because it looks reviewed.
2. **Preserve everything you cannot map**, under a namespaced `metadata.annotations` key. Lossy conversion is how a portable format becomes a one-way door.
3. **Set `writeback: propose`** on converted profiles. The source format had no writeback concept, so nobody has agreed to automatic writes.
4. **Label the result `imported`** and record the source in `metadata.annotations`. Trust comes from the discovery root, and a converted file has not earned any yet.

## Feature support matrix

| Feature | MagAgent (today) | Loro (today) | Claude Code | OAP |
| --- | --- | --- | --- | --- |
| Named agent definitions | yes | no | yes | yes |
| Markdown + frontmatter | yes | no | yes | yes |
| Model selection | yes | n/a | yes | yes |
| Capability tier routing | partial | yes (config) | no | yes |
| Tool allow/deny | yes | n/a | yes | yes |
| Permission narrowing | partial | yes (config) | partial | yes |
| MCP declaration | separate | separate | separate | yes |
| Skill references | separate | separate | separate | yes |
| Attached context | no | no | partial | yes |
| Composition (`extends`) | no | no | no | yes |
| **Persistent learned state** | no | no | no | **yes** |
| **Session-end writeback** | no | no | no | **yes** |
| **Revision history** | no | no | no | **yes** |
| **Self-modification boundary** | n/a | n/a | n/a | **yes** |

The bottom four rows are the reason the format exists. Everything above them is table stakes that OAP standardizes so those four have somewhere to live.
