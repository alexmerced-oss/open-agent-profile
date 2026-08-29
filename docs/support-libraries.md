# OAP support libraries

The repository contains support implementations for Python, TypeScript, Go,
Rust, and Java. They share the OAP 1.0 schemas, security invariants, RFC 8785
identity rules, and portable conformance fixtures.

Support-library release 1.0.4 is coordinated across all five languages. See the [release checklist](release-1.0.4.md) and [internal security review](security-review-1.0.4.md).

| Language | Runtime | Package or module | Documentation |
| --- | --- | --- | --- |
| Python | 3.10+ | `open-agent-profile` | This guide plus `oap.validate` and `oap.apply` |
| TypeScript | Node.js 20+ | `open-agent-profile` | [`typescript/README.md`](../typescript/README.md) |
| Go | 1.26+ | `github.com/alexmerced-oss/open-agent-profile` | This guide and Go package comments |
| Rust | 1.85+ | `open-agent-profile` | [`rust/README.md`](../rust/README.md) and docs.rs |
| Java | 17+ | `io.github.alexmercedcoder:open-agent-profile` | [`java/README.md`](../java/README.md) and the generated Javadocs |

## Python

```console
python -m pip install open-agent-profile
oap-validate --strict reviewer.agent.yaml
oap-apply reviewer.agent.yaml learned.delta.yaml --approve --dry-run
```

`oap.validate` exposes strict parsing, embedded-schema validation, security
checks, path containment, and profile/spec digests. `oap.apply` provides
conflict-safe state operations, retention, serialization, and atomic writeback.

## TypeScript

```console
npm install open-agent-profile
npx oap-validate --digest reviewer.agent.yaml
```

The dual ESM/CommonJS package adds composition, authority narrowing, normative
prompt rendering, and typed delta APIs. See its [language README](../typescript/README.md).

## Go

```console
go get github.com/alexmerced-oss/open-agent-profile@v1.0.4
```

```go
profile, err := oap.Load("reviewer.agent.yaml")
if err != nil { return err }
report := oap.Validate(profile)
if !report.OK { return fmt.Errorf("invalid profile: %v", report.Errors) }
prompt, err := oap.RenderSystemPrompt(profile, oap.RenderOptions{})
```

The repository also provides `cmd/oap-validate` and `cmd/oap-apply`.

## Rust

```console
cargo add open-agent-profile@1.0.4
cargo install open-agent-profile --version 1.0.4
oap-validate --digest reviewer.agent.yaml
```

The crate provides every parsing, validation, composition, policy, rendering,
delta, retention, and persistence surface. See the [Rust README](../rust/README.md).

## Java

```xml
<dependency>
  <groupId>io.github.alexmercedcoder</groupId>
  <artifactId>open-agent-profile</artifactId>
  <version>1.0.4</version>
</dependency>
```

Version 1.0.4 is published on [Maven Central](https://central.sonatype.com/artifact/io.github.alexmercedcoder/open-agent-profile/1.0.4).
The Java source builds a library JAR, source and Javadoc JARs, and an executable
CLI JAR with `validate` and `apply` commands. See the [Java README](../java/README.md).

## Safety and conformance

All full implementations fail closed on unsupported versions, reject duplicate
keys and literal credentials, treat learned state as untrusted data, prevent
delta operations outside `/state`, preserve proposals for human review, and
match the shared digest and retention vectors. See the specification's
[conformance requirements](../spec/v1/conformance.md) for normative details.
