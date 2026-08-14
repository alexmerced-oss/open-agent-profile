# Getting started

## Install the reference tools

```bash
pip install open-agent-profile
```

Two commands come with it: `oap-validate` and `oap-apply`.

## Write a profile

Create `.agents/note-taker.agent.yaml` in your project:

```yaml
oap: "1.0"
kind: AgentProfile

metadata:
  name: note-taker
  description: Summarizes a conversation into dated notes. Use at the end of a working session.

spec:
  role:
    instructions: |
      You take notes. Given a conversation or a set of documents, produce a dated
      summary with three sections: decisions made, open questions, and next steps.

      Write plainly. Do not pad. If a section is empty, say so in one line rather
      than inventing content to fill it.
```

That is a complete, valid profile. Everything else in the format is optional.

```bash
oap-validate .agents/note-taker.agent.yaml
```

## Add the parts you actually need

Reach for these in roughly this order. Most profiles never need the rest.

**A model.** Name one exactly, or express a capability tier and let the harness route:

```yaml
  model:
    provider: anthropic
    id: claude-sonnet-5
    tier: advanced          # used when the exact model is unavailable
    parameters:
      temperature: 0.1
```

**A tool surface.** The default policy is `allowlist`, so an empty list means no tools:

```yaml
  tools:
    policy: allowlist
    allow: [read, search]
    deny: [shell, write]
```

Remember that this is a *request*. The harness gives you the intersection of this and its own policy. Listing `shell` does not grant `shell`.

**Personality, structured.** Keep the substance in `instructions` and use these for the parts tooling should be able to diff:

```yaml
  role:
    persona:
      tone: direct, collegial
      verbosity: terse
    constraints:
      - Do not edit files. Report only.
```

Constraints render after instructions, so they win on conflict.

**Writeback.** This is the part that makes a profile more than a config file:

```yaml
  lifecycle:
    writeback: propose      # off | propose | auto
    retention:
      max_facts: 100
      fact_ttl_days: 365
      eviction: least_recently_used
```

`propose` is the default and the right starting point: the agent proposes what it learned, you approve it.

## Run a session and update the profile

How you start a session depends on your harness. What happens at the end is the same everywhere: the session emits an `AgentStateDelta`, and the process that owns the file applies it.

```yaml
# session.delta.yaml
oap: "1.0"
kind: AgentStateDelta
target:
  name: note-taker
  revision: 1
session:
  id: sess_01JDQ4X
  harness: loro
summary: Learned that the team wants next steps first.
operations:
  - op: add
    path: /state/facts/-
    value:
      id: fact-order
      text: Put next steps at the top of the summary, not the bottom.
      confidence: 0.9
      source: user statement
```

See what it would do, without touching anything:

```bash
oap-apply .agents/note-taker.agent.yaml session.delta.yaml --dry-run
```

Then apply it. The `--approve` flag is what satisfies `writeback: propose`:

```bash
oap-apply .agents/note-taker.agent.yaml session.delta.yaml --approve
```

The profile is now at revision 2, with a `history` entry recording who approved what and when.

## What a session cannot do

Try adding this to the delta:

```yaml
operations:
  - op: replace
    path: /spec/tools/allow
    value: [read, search, shell]
```

It is rejected. Operations may only touch `/state`. An agent that wants a wider tool surface has to ask, in a `proposals` block, with a rationale:

```yaml
proposals:
  - path: /spec/tools/allow
    op: replace
    value: [read, search, shell]
    rationale: Could not run the test suite it was asked to fix.
```

Proposals are surfaced to you and never applied automatically, including under `writeback: auto`. That boundary is the reason writeback is safe to leave on.

## Next

- [Field reference](field-reference.md) for what every field does
- [State and memory](state-and-memory.md) for what belongs in `state` and what does not
- [Implementers guide](implementers-guide.md) if you are adding OAP support to a harness
