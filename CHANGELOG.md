# Changelog

All notable changes to the Open Agent Profile specification and reference tools.

The specification and the reference implementation share a version number at 1.0. If they diverge later, the specification version in the `oap` field is the one that matters for conformance.

## [Unreleased]

## [1.0.4] - 2026-08-29

- Aligned Python, TypeScript, Rust, and Java package metadata for a coordinated 1.0.4 release; the normative OAP maintenance baseline remains 1.0.1.
- Added CI-verified machine-readable conformance results for Loro, MagAgent, and Merced-AI, including complete required-ID checks for every claimed level.
- Documented the distinction between self-declared ecosystem evidence and independent certification.
- Added a release checklist and internal security review for the coordinated support-library release.

## [1.0.3] - 2026-08-28

- Added a Java 17 support library with all encodings, schema and security validation, RFC 8785
  digests, verified inheritance, narrowing, rendering, atomic deltas, retention, and both CLIs.
- Added shared-corpus Java tests, compiler linting, Javadocs, SpotBugs, and Maven package checks.
- Published the signed Java artifacts to Maven Central and added an opt-in release profile.

- Added a Rust 1.85+ support crate with all three encodings, schema and security validation,
  RFC 8785 digests, inheritance, policy narrowing, prompt rendering, atomic delta application,
  Python-compatible retention, safe persistence, and `oap-validate`/`oap-apply` CLIs.
- Added shared-corpus Rust tests, strict Clippy and rustdoc gates, MSRV verification, package
  verification, and dependency vulnerability auditing.
- Added a consolidated support-library guide covering Python, TypeScript, Go, Rust, and Java.

## [1.0.2] - 2026-08-28

- Added the `open-agent-profile` npm package under `typescript/` with YAML, JSON, and Markdown
  parsing; schema and security validation; RFC 8785 digests; inheritance; narrowing helpers;
  normative prompt rendering; and atomic state-delta application.
- Added exact Python/TypeScript digest vectors, fixture-driven TypeScript tests, Node 20 CI,
  ESM/CommonJS distribution smoke tests, and npm package verification.
- Added the root Go module plus `oap-validate` and `oap-apply` commands with Go 1.26 support,
  all three encodings, schema and security validation, RFC 8785 digests, inheritance, policy
  narrowing, prompt rendering, atomic delta application, Python-compatible retention, race-tested
  conformance coverage, and Go vulnerability scanning.

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
