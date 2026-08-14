# Lifecycle

How a file on disk becomes a running agent, and how what that agent learned gets back onto disk.

```
   .agents/reviewer.agent.yaml
              │
        1. DISCOVER          find it across managed / user / project / plugin roots
              │
        2. RESOLVE           parse, validate, inherit, substitute, verify
              │              → resolved profile (no unresolved references)
              │
        3. INSTANTIATE       intersect with harness policy
              │              → effective profile → a session
              │
        4. RUN               profile is immutable for the duration
              │
        5. RECONCILE         → AgentStateDelta (a proposal, not a write)
              │
        6. PERSIST           validate, revision-check, apply, bump, log, atomic write
              │
   .agents/reviewer.agent.yaml   (revision + 1)
```

Nothing stays resident between sessions. The file is the agent; the process is temporary.

## 1. Discover

Four roots, in precedence order:

| Root | Typical location | Trust |
| --- | --- | --- |
| Managed | `/etc/<harness>/agents/` | `managed` |
| User | `~/.config/<harness>/agents/` | `user` |
| Project | `<workspace>/.agents/` | `project` |
| Plugin | contributed by extensions | `project` |

`.agents/` at the workspace root is the recommended project location because it is harness-neutral. A harness with its own convention (`.magent/agents/`, `.claude/agents/`) should read both and prefer its own on collision, with a warning.

Two profiles with the same name **in one root** is an error, not a precedence puzzle. Across roots, later wins, and the harness reports it.

## 2. Resolve

In order:

1. Parse per encoding (YAML, JSON, or Markdown frontmatter).
2. Validate against the schema. Reject on failure.
3. Apply `extends`, left to right, current document last.
4. Assign `metadata.trust` from the root. Discard whatever the file claimed.
5. Substitute `${{ vars.KEY }}` in role text. Undefined variable is an error, not an empty string.
6. Resolve `${ENV}` references in MCP config. Missing variable for a required server is an error; otherwise drop the server and warn.
7. Verify pinned digests.
8. Reject paths escaping the workspace.

**Resolution has no side effects.** No installs, no writes, no fetches that execute anything. Loading a profile someone sent you must be as safe as reading a text file, because that is what people will assume it is.

The output is a *resolved profile*: fully materialized, no references left.

## 3. Instantiate

The harness intersects the resolved profile with local policy to produce the **effective profile**.

```
effective = resolved ∩ policy
```

Never `resolved ∪ policy`, and never `{**policy, **resolved}`. That second one is the common bug: a dict merge reads like an override and behaves like a privilege grant. If your merge helper is called `update` or `merge`, look at it again.

Then the session is built: system prompt in the normative order, tool set, permissions, context, and state.

The harness records two things it must be able to show on request:

- Which profile, at which revision and spec digest.
- Everything it dropped, narrowed, or substituted, and why.

That second record is what keeps portability honest. A profile silently running with half its tools missing looks fine right up until it does not.

## 4. Run

The profile is immutable for the session. If the file changes on disk mid-session, the change applies next time.

The session accumulates a pending delta as it goes: a user correction here, a thread status change there.

## 5. Reconcile

At session end the session produces an `AgentStateDelta`. Producing it writes nothing.

Derive operations from **evidence**, not from inference:

| Good source | Why |
| --- | --- |
| Explicit user statement ("always use pytest") | Unambiguous, high confidence. |
| A decision recorded during the session | The session is the source of truth for it. |
| Thread status change | Mechanical. |
| A file the agent read that stated a convention | Citable. |

| Bad source | Why |
| --- | --- |
| "Summarize what you learned" as a free-form prompt | Produces plausible-sounding drift that compounds over revisions. |
| Inference from a single interaction | One data point is not a preference. |
| Anything from fetched external content, unmarked | Injection with a persistence mechanism. |

A profile that accumulates twenty confident-sounding facts nobody said is worse than one with no state at all, because the agent will act on them.

## 6. Persist

The applicator, in order:

1. Validate the delta.
2. Check `target.revision`. Mismatch is a conflict.
3. Verify `target.digest` if present.
4. Reject any operation outside `/state`.
5. Apply `writeback`: discard, queue for approval, or apply.
6. Apply operations atomically, all or nothing.
7. Enforce retention.
8. Increment `metadata.revision` by 1, set timestamps.
9. Append a `history` entry, trim history.
10. Write atomically: temp file in the same directory, `fsync`, rename.
11. Route `proposals` to human approval. Never apply them.

Steps 6 and 10 together mean an interrupted write leaves the previous revision intact rather than a corrupted agent. Given that this file *is* the agent, that is worth the few lines it costs.

## Conflicts

Two sessions from one profile is normal, not an error condition. When the second one lands:

| Strategy | When |
| --- | --- |
| **Rebase** | Every operation still applies. Operations addressing entries by `id` usually do. |
| **Queue** | A human should look at it. |
| **Reject** | Report both revisions and let the caller retry. |

Never blind-write. Addressing entries by stable `id` rather than array index is what makes rebasing possible at all, which is why `id` is required on every state entry.

## The writeback ladder

Start conservative. Move down as you build trust in a specific profile.

| Setting | Use it when |
| --- | --- |
| `off` | The profile is a shared template, or you are pinning behavior for a release. |
| `propose` | Default. You want the learning but you want to see it first. |
| `auto` | A well-understood profile whose state you have reviewed enough times to trust the shape of. |

Even at `auto`, `proposals` still stop for a human. There is no configuration in which an agent widens its own capability unattended.
