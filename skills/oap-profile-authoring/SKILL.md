---
name: oap-profile-authoring
description: >-
  Create or refine a reviewable Open Agent Profile draft from a natural-language
  request. Use when a user asks to create, design, generate, or update an agent
  profile, or when an agent proposes a specialized subagent profile for a task.
---

# Authoring Open Agent Profiles

Turn the request into OAP data, never executable setup. A generated profile is
a proposal until the user or an explicit harness policy approves it.

## Workflow

1. Read the harness's OAP schema and its local catalog of providers, models,
   tools, skills, MCP servers, memory stores, hooks, and base profiles.
2. Draft one complete `AgentProfile` JSON object. Prefer a narrow specialist
   with a useful routing description, explicit objectives, constraints, and
   conservative permissions.
3. Reference only catalog entries. Never invent a tool, skill, MCP command,
   hook, credential, executable, package, or environment secret.
4. Validate the complete document with the canonical OAP validator.
5. Show requested and effective authority, unresolved references, validation
   findings, and a concise rationale outside the OAP document.
6. A direct user request to create the profile may proceed through the
   harness's normal confirmation and save boundary. If the agent independently
   decides a new subagent would help, create a proposal; do not activate it
   unless local policy explicitly permits autonomous profile activation.

## Safe defaults

- `metadata.revision`: `1`
- `spec.lifecycle.writeback`: `propose`
- consequential permissions: `ask` or `deny`
- no tools, skills, MCP servers, hooks, delegation, or network access unless the
  request needs them and the harness catalog contains them
- no initial `state` claims inferred from the prompt
- no `history` entry before the first persisted revision

The profile may request less authority than the harness grants. It can never
widen harness or parent-agent policy.

## Output

Return a proposal envelope with these fields:

```json
{
  "document": {},
  "rationale": {},
  "warnings": [],
  "unresolved_references": [],
  "model": "",
  "prompt_digest": "sha256:..."
}
```

Only `document` is the OAP profile. Do not insert rationale, warnings, model
provenance, or proposal state into the OAP document as unknown fields.

## Universal profile location

Use `~/.agentprofiles/` for a harness-neutral user profile and `.agents/` for a
project profile. Harness-native roots may also be offered, but collisions must
be shown and the native user root takes precedence over the universal user
root.
