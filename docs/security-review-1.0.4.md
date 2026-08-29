# Internal security review: support libraries 1.0.4

Date: 2026-08-29. Scope: the Python, TypeScript, Go, Rust, and Java OAP parsers, validators, composition helpers, policy helpers, renderers, delta applicators, retention logic, persistence helpers, and CLIs. This is a maintainer review, not an independent security audit.

## Controls exercised

| Threat | Required behavior | Evidence |
| --- | --- | --- |
| Parser ambiguity | YAML 1.2 scalar behavior and duplicate-key rejection | Cross-language conformance tests |
| Future-version confusion | Unsupported versions fail closed | Cross-language conformance tests |
| Credential persistence | Literal credentials and secret-like values are rejected | Invalid fixture corpus |
| Workspace escape | Traversal and out-of-root resource paths are rejected | Invalid fixture corpus and validator tests |
| Capability escalation | Profile requests are narrowed against harness policy | Cross-language narrowing tests |
| Persistent prompt injection | State is rendered as untrusted content and cannot alter `spec` authority | Behavioral state-authority tests |
| Self-modifying contract | Delta operations outside `/state` are rejected; widening changes remain proposals | Invalid delta corpus and proposal-gate tests |
| Partial/corrupt writeback | Deltas apply to a copy, conflicts fail, and persistence replaces atomically | Atomicity, conflict, and write-failure tests |
| Unbounded retained state | Expiry, caps, ordering, and pinned-entry behavior are enforced | Cross-language retention vectors |
| Inheritance substitution | Base identity/state do not overwrite child identity/state; pins use stable spec digests | Composition and digest tests |

The three ecosystem reports are now checked against the result schema and the complete set of requirement IDs for each claimed level. This verifies record consistency, not the truth of a self-declared result.

## Residual risks

- A host harness must correctly implement the final permission intersection and preserve the preamble/postamble authority boundary.
- Profile discovery and remote inheritance resolvers require their own symlink, SSRF, path-containment, signature, digest, size, and timeout controls.
- Atomic replacement cannot by itself provide distributed locking across machines; callers need an external coordination strategy for shared profiles.
- No coverage-guided fuzzing, cryptographic signature envelope, or independent penetration test has been completed.

No release-blocking defect was found in the reviewed support-library surfaces. Independent implementation and security review remain gates for advancing OAP beyond Draft.
