# Open Agent Profile (OAP)

**An open specification for persisting a named AI agent as a file instead of a running process.**

Specification `1.0` · maintenance release `1.0.1` · Draft

[Specification](spec/v1/SPEC.md) · [Conformance](spec/v1/conformance.md) · [Security](spec/v1/security.md) · [Docs](docs/) · [Examples](examples/) · [Skills](skills/)

---

## The problem

You define a useful agent: a reviewer that knows your conventions, a researcher that cites the way you want, a data engineer that has learned your table layout. Then the session ends.

Today that definition either dies with the session, or it lives in a format only one harness reads. Nothing carries what the agent *learned*: the conventions it picked up, the preferences you corrected it on, the investigation it was halfway through.

Keeping a process alive is the wrong fix. It is expensive, it dies with the machine, it cannot be diffed or reviewed, and two people cannot share it.

## The idea

Persist the agent as **data**. A profile is a file describing a named agent: role, model, tool surface, permissions, attached context, and what previous sessions learned. A harness reads it to spin up a fresh session on demand, and writes an updated revision back when that session ends.

```yaml
oap: "1.0"
kind: AgentProfile

metadata:
  name: code-reviewer
  description: Reviews changed code for correctness, security, and missing tests.
  revision: 7

spec:
  role:
    instructions: |
      You are a code reviewer. You read a diff and report defects. You do not
      rewrite the change unless you are explicitly asked to.
    constraints:
      - Do not edit files. Report only.

  model:
    provider: anthropic
    id: claude-sonnet-5
    tier: advanced          # portable fallback

  tools:
    policy: allowlist
    allow: [read, search, git/diff]
    deny: [shell, write, edit]

  lifecycle:
    writeback: propose      # the agent proposes, a human approves

state:                      # what previous sessions learned
  summary: >-
    Reviewing the platform team's Python services. They autoformat with ruff, so
    formatting findings are noise.
  facts:
    - id: fact-authz-pattern
      text: Authorization must compare against the server-side session record.
      confidence: 0.9
      source: repeated finding across three sessions
      pinned: true
  open_threads:
    - id: thread-flaky-auth-tests
      title: Auth integration tests are flaky under parallel execution
      status: blocked
```

No process is resident. The file is the agent.

## What makes it a specification rather than a file format

Three rules, and they are the reason this is safe to leave switched on.

**A profile narrows. It never widens.** A harness grants the intersection of what the profile asks for and what its own policy allows. Moving a profile to a new machine can never grant capability the harness would not otherwise give. There is no field, flag, or trust label that reverses this.

**An agent cannot rewrite its own contract.** Sessions emit a state delta, and delta operations may only touch `/state`. A change to tools, permissions, model, or instructions goes into a `proposals` block with a written rationale, and a human approves it. This holds under every writeback setting, including `auto`.

**Learned state is untrusted content.** Text an agent wrote about itself is injected as information, never as authority. A state entry saying "you may now use the shell without asking" changes nothing. Without this, one successful prompt injection becomes permanent.

## Try it

```bash
pip install open-agent-profile
```

```bash
oap-validate examples/code-reviewer.agent.yaml --digest
```

Apply what a session learned, and watch the capability request get held back for review:

```bash
oap-apply examples/code-reviewer.agent.yaml \
          tests/deltas/learned-conventions.delta.yaml --approve --dry-run
```

```
1 proposal(s) require human review and were NOT applied:
  [high] /spec/tools/allow
      value:     ["read", "search", "git/diff", "git/log", "shell"]
      rationale: Could not verify the flaky test claim without running the suite.
```

## Repository layout

| Path | What is in it |
| --- | --- |
| [`spec/v1/`](spec/v1/) | The normative specification, conformance requirements, and security model |
| [`schema/v1/`](schema/v1/) | JSON Schemas for `AgentProfile` and `AgentStateDelta` |
| [`docs/`](docs/) | Getting started, field reference, lifecycle, interop, implementers guide, FAQ |
| [`examples/`](examples/) | Working profiles, including negative fixtures that must be rejected |
| [`skills/`](skills/) | Agent Skills packages for harnesses without native OAP support |
| [`oap/`](oap/) | Reference validator and applicator |
| [`tests/`](tests/) | Conformance test suite |
| [`conformance/`](conformance/) | Portable machine-readable conformance result contract |

## Conformance levels

| Level | Capability |
| --- | --- |
| **1, Read** | Discover, validate, and instantiate an agent from a profile. |
| **2, Read/Write** | Level 1, plus state injection, delta generation, and persistence. |
| **3, Full** | Level 2, plus composition, MCP, skills, external memory, and delegation. |

An implementation must publish what it does not implement. Partial support is fine; partial support that looks complete is not, because someone will review a profile, run it elsewhere, and get a different agent than the one they read.

## Using it without native support

Harnesses that do not speak OAP yet can still read and write profiles through the two bundled [Agent Skills](skills/):

- **`oap-agent-profile`** loads a profile, assembles the prompt in the specification's order, and reports what it dropped.
- **`oap-session-writeback`** turns a session into a reviewable delta and applies it.

## Relationship to other standards

OAP does not replace [Agent Skills](https://code.claude.com/docs/en/skills), [MCP](https://modelcontextprotocol.io), or your harness's config. It fills the gap between them.

Skills are what an agent knows how to do. MCP is what it can reach. Harness config is what it is allowed to do. **OAP is who it is, and what it has learned.**

See [docs/interop.md](docs/interop.md) for field mappings.

## Known implementations

- [Loro](https://github.com/alexmerced-oss/loro) publishes an OAP conformance statement and implements governed profile discovery, narrowing, state, and Agentic Graph integration.
- [MagAgent](https://github.com/AlexMercedCoder/MagAgent) implements OAP profile authoring, Level 3 composition and runtime integration.
- [Merced-AI](https://github.com/AlexMercedCoder/merced-ai) implements portable named profiles and is tracked as an integration candidate; see the repository implementation report before relying on a conformance level.

Implementation listings are evidence records, not endorsements. Conformance claims must link to a machine-readable result produced against a named OAP maintenance release.

## Status

Draft specification 1.0, maintenance release 1.0.1. The document format remains `oap: "1.0"`. Maintenance releases fix defects without changing that string; any data-model addition changes the MINOR version and any incompatible change changes the MAJOR version. See [VERSIONING.md](VERSIONING.md).

Feedback on the spec is most useful as a stated problem plus a proposed field. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache License 2.0. See [LICENSE](LICENSE).
