# Implementation report

This report tracks public implementation evidence for OAP 1.0 maintenance release 1.0.1. A repository listing is not a conformance certification. Each implementation must publish a result matching `conformance/result.schema.json` and identify the exact OAP fixture revision it ran.

| Implementation | Repository | Observed surface | Verified status |
| --- | --- | --- | --- |
| Loro | https://github.com/alexmerced-oss/loro | Discovery, effective-policy narrowing, prompt rendering, state and deltas, inheritance, Agentic Graph integration | Existing self-declared Level 3 statement; rerun required against 1.0.1 |
| MagAgent | https://github.com/AlexMercedCoder/MagAgent | Profile authoring, discovery, narrowing, deltas, inheritance, skills, MCP and delegation integration | Existing self-declared Level 3 checks; rerun required against 1.0.1 |
| Merced-AI | https://github.com/AlexMercedCoder/merced-ai | Named profile and session management | No OAP conformance level verified |

The standards repository does not certify implementations. It records reproducible evidence and unresolved deviations. Independent implementation review remains required before OAP advances beyond Draft.

The detailed 2026-08-27 migration audit is maintained in the AGS repository at https://github.com/AlexMercedCoder/agentic-graph-spec/blob/v1.0.1/implementation-audits/README.md.
