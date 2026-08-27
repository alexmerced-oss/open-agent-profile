# Versioning and publication

OAP has three related versions:

1. The document data-model version is the `MAJOR.MINOR` string in `oap`.
2. The specification maintenance release is `MAJOR.MINOR.PATCH` and may correct wording, tests, schemas, and reference tools without changing the document model.
3. Each implementation has its own version and declares which OAP maintenance release it was tested against.

OAP 1.0 maintenance release 1.0.1 accepts `oap: "1.0"`. Unsupported document minors and majors fail closed. A new optional standard field requires OAP 1.1 and a separately published schema. An incompatible field or behavior requires OAP 2.0.

Every specification release must include an immutable Git tag, release notes, schemas, source archive, conformance fixtures, SHA-256 checksums, and a successful CI run. Canonical schema URLs must resolve to immutable release content; the repository copy is authoritative until `openagentprofile.org` is operational.
