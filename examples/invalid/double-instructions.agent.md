---
# INVALID: violates SPEC 2.1 / L3-G1. In Markdown encoding the body IS
# spec.role.instructions. Supplying it in frontmatter as well is ambiguous.
oap: "1.0"
kind: AgentProfile
metadata:
  name: doubled
  description: Supplies instructions in both frontmatter and body.
spec:
  role:
    instructions: Instructions in the frontmatter.
---

Instructions in the body.
