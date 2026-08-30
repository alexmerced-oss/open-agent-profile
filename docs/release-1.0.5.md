# Coordinated 1.0.5 release checklist

This support-library release advances the OAP 1.0 normative maintenance baseline to 1.0.2. The
document model and `oap: "1.0"` version string do not change.

## Required gates

- Verify the universal `~/.agentprofiles/` discovery wording, native-root precedence, collision
  reporting, and the review-first profile-authoring skill.
- Run `python tools/verify_conformance_results.py`, `python tools/verify_release_versions.py`, and
  `python -m pytest -q`.
- Build and verify Python, TypeScript, Go, Rust, and Java packages with the same commands documented
  in the 1.0.4 checklist.
- Confirm version 1.0.5 is unused on PyPI, npm, crates.io, Maven Central, and as a Git tag.
- Publish Python, npm, crates.io, Maven Central, then push the signed `v1.0.5` tag that publishes Go.
- Verify clean registry installs, checksums, package contents, and the GitHub release.
