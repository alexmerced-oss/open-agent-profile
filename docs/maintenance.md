# Open Agent Profile maintenance tracker

Audited 2026-11-24 against checkout `7cb1f97`. All 74 Python tests passed. The mixed-input check below ran separately against the reference CLI.

Claim a row by entering your name or task ID and setting the status to **In progress**. Use **Blocked** with the missing dependency if needed. Set **Done** after the acceptance check passes and record the fix commit or PR, the command or manual check, and its result. If a newer revision already resolves the item, record that evidence and close it. Keep status in this table.

| ID | Priority | Status | Owner | Fix and verification |
| --- | --- | --- | --- | --- |
| OAP-01 | P1 | Open | — | — |

### OAP-01 · Fail validation when an explicitly requested file is missing

**Reproduced.** `python -m oap.validate --strict examples/code-reviewer.agent.yaml missing.agent.yaml` warns, reports `1/1 documents valid`, and exits 0. `collect()` in [oap/validate.py](../oap/validate.py) drops missing paths before reports are built.

**Acceptance check.** Exercise valid-only, missing-only and mixed batches in text and JSON modes. A missing input must be named and make the command fail, including under --strict and --expect-invalid. Negative fixtures should pass only for documents actually read and rejected.
