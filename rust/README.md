# Open Agent Profile for Rust

Native Rust support for Open Agent Profile 1.0, at parity with the Python,
TypeScript, and Go support libraries:

- YAML 1.2, JSON, and Markdown parsing with duplicate-key rejection;
- embedded profile and delta schemas plus security validation;
- RFC 8785 profile and spec digests;
- profile composition with cycle, depth, revision, and digest pinning;
- permission narrowing and tool intersection;
- normative prompt assembly with untrusted-state labeling;
- atomic, conflict-safe state delta application and Python-compatible retention;
- atomic persistence and all three output encodings; and
- the `oap-validate` and `oap-apply` CLIs.

The MSRV is Rust 1.85.0 and the crate uses edition 2024.

```rust
use open_agent_profile::{load, profile_digests, validate};

let profile = load("reviewer.agent.yaml")?;
let report = validate(&profile, None);
assert!(report.ok, "{:?}", report.errors);
println!("{:?}", profile_digests(&profile)?);
# Ok::<(), Box<dyn std::error::Error>>(())
```

```console
cargo install open-agent-profile --version 1.0.4
oap-validate --digest reviewer.agent.yaml
```
