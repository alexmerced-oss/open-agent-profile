# Changelog

All notable changes to the Open Agent Profile specification and reference tools.

The specification and the reference implementation share a version number at 1.0. If they diverge later, the specification version in the `oap` field is the one that matters for conformance.

## [1.0.1] - 2026-08-27

Normative errata and standards-hardening release. The document version remains `oap: "1.0"`.

- Replaced the incomplete bespoke digest algorithm with RFC 8785 JCS and added interoperability vectors.
- Defined spec-digest metadata normalization so revision, update timestamp, and resolver-assigned trust do not invalidate pins after state-only writeback.
- Corrected YAML parsing to the YAML 1.2 boolean rules.
- Resolved the closed-schema/forward-minor contradiction by requiring unsupported minor versions to fail closed.
- Added governance, security reporting, versioning, implementation-report, and portable conformance-result documentation.
- Documented Loro, MagAgent, and Merced-AI as implementation references without treating listings as conformance endorsements.

## [1.0.0] - 2026-08-14

Initial draft.

### Specification

- `kind: AgentProfile`: `metadata`, `spec`, `state`, `history`, and `extends`.
- `kind: AgentStateDelta`: `operations` scoped to `/state`, and `proposals` for anything that would change the instantiation contract.
- Three encodings: YAML, JSON, and Markdown with YAML frontmatter, all logically identical.
- Profile digest and spec digest, over canonical JSON. Pinning uses the spec digest so that learning does not invalidate a pin.
- Six-phase lifecycle: discover, resolve, instantiate, run, reconcile, persist.
- Normative system-prompt assembly order, with harness rules first and last.
- Three security boundaries: privilege narrows and never widens; agents cannot write `/metadata` or `/spec`; learned state is untrusted content.
- Conformance levels 1 (Read), 2 (Read/Write), and 3 (Full), with a behavioral test list.

### Reference implementation

- `oap-validate`: schema validation plus the structural rules JSON Schema cannot express (literal secret detection, workspace escape, Markdown encoding, version gating, pointer scope).
- `oap-apply`: revision checking, `/state` scope enforcement, atomic operation application, retention, revision bump, history append, and atomic file replacement.
- 58 conformance tests covering the fixture layer and the Level 2 behavioral requirements.

### Skills

- `oap-agent-profile`: load and run as a profile on a harness without native support.
- `oap-session-writeback`: turn a session into a reviewable delta and apply it.

### Known gaps

- Signature envelopes for `imported` profiles are reserved for a future version. Pin by spec digest in the meantime.
- `kind: AgentProfileBundle` and `AgentProfileIndex` are reserved and currently rejected.
- Delta operations `move`, `copy`, and `test` are reserved.
- The reference applicator rejects on revision conflict rather than rebasing. Rebasing is permitted by the spec and left to harnesses that want it.
