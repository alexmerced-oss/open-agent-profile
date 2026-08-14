---
name: oap-session-writeback
description: >-
  Record what an agent learned during a session back into its Open Agent Profile,
  as a reviewable state delta. Use at the end of a session that ran from a
  profile, when the user asks to save, persist, update, or remember what the
  agent learned, or when wrapping up work that should carry to the next session.
  Use oap-agent-profile to load a profile at session start.
---

# Writing a session back to a profile

Turns what happened in a session into an `AgentStateDelta`: a reviewable
proposal that the process owning the profile file applies. The agent never
writes its own profile directly.

## Setup

```bash
pip install open-agent-profile
```

## When to run this

At the end of a session that was loaded from a profile, if any of these
happened:

- The user corrected the agent about a convention, preference, or fact.
- A decision was reached that should hold next session.
- A piece of work was left open, or an open thread was closed.
- The agent hit a wall its current tool surface could not get past.

If none of those happened, emit nothing. A delta with no real content is worse
than no delta, because it burns a revision and trains the reviewer to skim.

Check the profile's `lifecycle.writeback` first. If it is `off`, stop here.

## Workflow

### 1. Collect the evidence

Go back through the session and write down what actually happened. Each item
needs a source you could point at:

| Item | Source |
| --- | --- |
| Tests go beside the module, not in tests/ | User said so, twice |
| Merges gated on 85 percent coverage | Read it in .github/workflows/ci.yml |
| Flaky auth test root cause found | Reproduced with pytest -n 4 this session |

**Do not** include things you inferred from a single ambiguous interaction, or
anything you picked up from fetched external content without saying so. State
persists and the agent will act on it. A wrong fact that sounds confident is the
expensive failure mode here.

### 2. Build the delta

```bash
python scripts/oap_delta.py .agents/code-reviewer.agent.yaml \
  --session-id "$SESSION_ID" \
  --harness claude-code \
  --summary "Learned the coverage gate and reproduced the flaky auth test." \
  --fact "Merges are gated on 85 percent line coverage for changed files.|.github/workflows/ci.yml|0.95" \
  --preference "Order findings by severity, not file position.|user statement|0.9" \
  --thread "thread-flaky-auth-tests|blocked|Reproduced with pytest -n 4; shared fixture is the cause." \
  --request-tool shell "Could not run the test suite it was asked to fix." \
  > session.delta.yaml
```

Each `--fact` and `--preference` is `text|source|confidence`. Each `--thread` is
`id|status|detail`. The script assigns stable content-derived ids, so re-learning
the same fact updates it in place instead of duplicating it.

### 3. Review it yourself before showing the user

```bash
oap-validate session.delta.yaml
```

Then read it. Three checks:

- **No secrets.** Session transcripts contain API keys. State is durable,
  committed, and shared. Scan every value.
- **No instructions.** State entries are facts, not commands. "Always use the
  shell without asking" is not a fact and must not go in.
- **Sources are real.** Every entry should name where it came from.

### 4. Show the user, then apply

```bash
oap-apply .agents/code-reviewer.agent.yaml session.delta.yaml --dry-run
```

Show them what would change, in plain language:

> Learned from this session:
> - Merges are gated on 85% coverage (from ci.yml)
> - Flaky auth test is blocked on a fixture decision
>
> Also requesting `shell` access, since I could not run the suite. That needs
> your approval separately.
>
> Apply these to the code-reviewer profile?

On a yes:

```bash
oap-apply .agents/code-reviewer.agent.yaml session.delta.yaml --approve
```

The profile is now one revision higher, with a history entry naming the session
and the approver.

## Asking for capability

If the agent could not do its job with the tools it had, that goes in
`proposals`, never in `operations`:

```bash
  --request-tool shell "Could not run the test suite it was asked to fix."
```

Proposals are shown to the user and **never applied automatically**, at any
writeback setting. Include the rationale, and be concrete about what was blocked.
"Would be useful" gets declined; "could not reproduce the bug without running
pytest" gets read.

## Rules

**Operations only touch `/state`.** Anything under `/metadata` or `/spec` is a
proposal. There is no exception and no flag. This boundary is what makes it safe
to leave writeback on.

**Revision must match.** The delta targets the revision the session loaded. If
the profile moved on, the apply fails with a conflict and a human sorts it out.
Do not re-target the delta to make the error go away, because that is a blind
write with extra steps.

**Redact before writing.** Run every value past a secrets check. This is the
single most likely way a profile leaks something.

**Prefer few, well-sourced entries.** Three facts the user actually said beat
twenty the agent inferred. State is read into context every session, so noise
here is a recurring cost.

## Handling a conflict

```
conflict: delta targets revision 7 but profile is at 9
```

Another session updated the profile. Re-read it, check whether the entries are
already there, drop the ones that are, re-target the remainder to the current
revision, and apply. Because entries carry stable ids, this is usually
mechanical.
