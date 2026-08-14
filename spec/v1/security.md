# OAP v1 Security Model

Normative. Companion to [SPEC.md](SPEC.md) §7.

## 1. What a profile is, in security terms

A profile is a **file that describes a privileged process**. People will share them, commit them, fork them, generate them, and email them. Some will come from a teammate, some from a package registry, some from a model.

So the governing assumption is: **an attacker can author a profile that a user will load.**

Every rule below follows from that. If a profile could grant capability, install software, execute a command, or edit its own instructions, then sharing one would be equivalent to sharing a shell script that runs as you, and the format would be unsafe by design.

## 2. Trust boundaries

```
┌─────────────────────────────────────────────────────────┐
│  Harness policy  (trusted, machine-owned)               │
│  ┌───────────────────────────────────────────────────┐  │
│  │  metadata + spec  (semi-trusted, human-authored)  │  │
│  │   - reviewed by a human, never by an agent        │  │
│  │   - can only narrow the layer above               │  │
│  │  ┌─────────────────────────────────────────────┐  │  │
│  │  │  state  (UNTRUSTED, agent-authored)         │  │  │
│  │  │   - context only, never authority           │  │  │
│  │  └─────────────────────────────────────────────┘  │  │
│  │  ┌─────────────────────────────────────────────┐  │  │
│  │  │  context.documents  (UNTRUSTED, external)   │  │  │
│  │  └─────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

Authority flows inward only. No inner layer can raise the ceiling of an outer one.

## 3. Threats and required mitigations

### T1: Privilege escalation through a shared profile

*A profile requests tools, filesystem roots, or network access the user's policy does not grant.*

**Mitigation (MUST).** The narrowing rule, SPEC §7.2. Effective capability is the intersection of profile request and harness policy, computed as the minimum on the `deny < ask < allow` ordering. There is no override, no trust label, and no flag that reverses this.

Implementations should test this directly rather than assuming it. The common failure is a merge helper that treats the profile as an update to a defaults object, which produces union semantics without anyone deciding to.

### T2: Code execution by loading a profile

*A profile carries a command, script, hook body, or install directive that runs on load.*

**Mitigation (MUST).**

- Profiles carry no executable content. `lifecycle.on_start` and `on_end` reference **named hooks already registered with the harness**, with string parameters only.
- Resolution is side-effect free: no installs, no writes, no network fetches that execute anything.
- `spec.tools.skills` and `extends` reference content; they never fetch and run it. A missing skill is reported, not installed.
- `mcp_servers[].command` and `args` are the only executable references. They are declarations of intent, and launching them is gated separately.

**Mitigation (SHOULD).** Do not auto-launch MCP servers declared by a `project` or `imported` profile without explicit confirmation showing the command line.

### T3: Credential exfiltration

*A profile carries or induces disclosure of secrets.*

**Mitigation (MUST).**

- `mcp_servers[].env` values are same-name `${VARIABLE}` references. Literals are rejected at resolve time.
- `mcp_servers[].headers` values match `^(Bearer )?\$\{[A-Z][A-Z0-9_]{0,63}\}$`. Literals are rejected.
- A resolver SHOULD scan the entire document for high-entropy strings matching known credential patterns and refuse to load on a match.

**Mitigation (MUST) for the write path.** Delta generation MUST run the harness's secret-redaction pass over every operation value before the delta is written. Session transcripts contain keys. State written from a transcript is a durable, version-controlled, frequently-shared copy of those keys.

### T4: Persistent prompt injection through state

*Content written into `state` in one session changes agent behavior in every future session.*

This is the threat unique to this format, and the one most likely to be implemented wrong.

**Mitigation (MUST).**

- State is injected as untrusted, agent-authored content, clearly delimited, at position 7 of the assembly order, never as system authority (SPEC §3.2, §3.9).
- Instruction-like text in state that would change tool access, permissions, safety behavior, or agent identity MUST be ignored.
- Deltas cannot write outside `/state`. A session that ingests a poisoned web page still cannot edit `spec.role.instructions`.
- Under `writeback: propose`, a human sees the operations before they persist.

**Mitigation (SHOULD).** Record `source` on every state entry and surface it in review UIs. "The agent inferred this from a fetched page" and "the user said this out loud" deserve different scrutiny, and only the entry's provenance can tell them apart.

### T5: Silent capability drift

*A profile runs on a harness that quietly ignores half of it, and the user believes they are running the agent they reviewed.*

**Mitigation (MUST).** Record every dropped, narrowed, or substituted field with a reason (L1-I9), and fail rather than degrade when a `required: true` skill or memory store is unavailable (L1-I7).

**Mitigation (MUST).** Report which model resolution rule was applied when the exact model was not used (L1-I12). An agent reviewed on a frontier model and silently run on a small one is a different agent.

### T6: Path traversal and workspace escape

*`context.files`, `permissions.filesystem.*`, or `context.working_directory` reach outside the workspace.*

**Mitigation (MUST).** Resolve and reject at **resolve time**, before instantiation, not at tool-call time. Reject absolute paths unless the harness explicitly permits them, and reject any path whose normalized form escapes the workspace root. Resolve symlinks before the check.

### T7: Delegation escalation

*A constrained agent delegates to a less constrained one.*

**Mitigation (MUST).** A subagent's effective profile is intersected with the **parent's effective profile**, not with the harness default (L3-B1). `max_depth` and `max_concurrent` are enforced.

### T8: Concurrent write corruption

*Two sessions from one profile race, and the loser's changes vanish or the file is left half-written.*

**Mitigation (MUST).** Revision checking with no blind writes (L2-A3), atomic all-or-nothing operation application (L2-A5), and atomic file replacement via temp-file-plus-rename with `fsync` (L2-A12).

### T9: Unbounded growth

*State grows until it crowds out the conversation or blows the context window.*

**Mitigation (MUST).** `lifecycle.retention` enforced by the applicator at write time, and `context.budget.max_state_tokens` enforced by the harness at injection time. Eviction drops whole entries, never partial ones, and never `pinned` ones.

### T10: Supply chain, for imported profiles

*A profile from a registry or a converted third-party definition changes between review and use.*

**Mitigation (SHOULD).** Pin by **spec digest** (SPEC §2.2), which covers `metadata` and `spec` but not `state`, so pinning survives the agent learning things. Verify at load. Label the profile `imported` and require review before first use.

## 4. Rules for implementers, condensed

Ten rules. If an implementation follows all ten, it is very hard to make it unsafe with a hostile profile.

1. Intersect, never merge, when combining profile requests with policy.
2. Assign trust from the discovery root, never from the file.
3. Reject literal secrets. Environment references only.
4. Resolve without side effects.
5. Inject state as untrusted content, and never let it change authority.
6. Restrict delta operations to `/state`. Route everything else to human approval.
7. Check the revision before writing. Never blind-write.
8. Write atomically, or do not write.
9. Enforce retention and context budgets at the boundary, not in the prompt.
10. Record and surface every drop, narrowing, and substitution.

## 5. Non-goals

OAP does not provide confidentiality for profile contents, authentication of profile authors (signature envelopes are reserved for a future version), sandboxing of tools the harness already grants, or protection against a compromised harness. A harness that ignores the narrowing rule cannot be made safe by anything in the file format.
