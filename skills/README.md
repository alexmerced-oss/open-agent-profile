# Skills

Agent Skills packages that give OAP support to harnesses that do not have it natively.

| Skill | What it does |
| --- | --- |
| [`oap-agent-profile`](oap-agent-profile/) | Discover, validate, and run as a profile. Assembles the system prompt in the specification's order, reports the requested capability surface, and injects learned state as untrusted content. |
| [`oap-session-writeback`](oap-session-writeback/) | Turn a session into a reviewable `AgentStateDelta`, then apply it. Routes capability requests into `proposals` where a human approves them. |
| [`oap-profile-authoring`](oap-profile-authoring/) | Turn a natural-language request into a conservative, schema-valid OAP profile proposal. Direct user requests may be saved after review; autonomous subagent ideas remain proposals until policy permits activation. |

## Installing

Both follow the [Agent Skills](https://code.claude.com/docs/en/skills) filesystem format: a directory whose name matches the `name` in `SKILL.md`. Copy the directory into your harness's skills root.

```bash
# Claude Code
cp -r skills/oap-agent-profile skills/oap-session-writeback skills/oap-profile-authoring ~/.claude/skills/

# A project-scoped skills directory
cp -r skills/oap-agent-profile skills/oap-session-writeback skills/oap-profile-authoring .agents/skills/
```

Both need the reference tools:

```bash
pip install open-agent-profile
```

## What these skills can and cannot do

They cover the parts of OAP that live in the agent's own behavior: reading a profile, adopting its role, treating state as information rather than instruction, and producing a well-formed delta at the end.

They **cannot** enforce the narrowing rule. That happens where a profile meets a policy engine, which is inside the harness. A skill can tell an agent to honor a profile's `shell: deny`, and a well-behaved agent will, but nothing stops a harness that grants shell from granting shell.

So these are a bridge, not a substitute. A harness that wants real OAP guarantees implements Level 1 natively. See [docs/implementers-guide.md](../docs/implementers-guide.md).
