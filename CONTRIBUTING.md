# Contributing

## What is most useful

**Implementations.** The fastest way to improve a specification is to build against it. If you add OAP support to a harness, open an issue with your conformance statement and what you found awkward. Awkwardness in an implementation is a specification bug, not an implementer problem.

**Problem reports.** "This field cannot express X" is more useful than "please add field Y". Lead with the case that does not fit.

**Negative fixtures.** If you find a document that should be rejected and is not, that is a bug in the schema or the validator. A pull request adding it to `examples/invalid/` or `tests/deltas/invalid/` with a comment naming the rule it violates is welcome on its own.

## Proposing a change to the specification

Every field is something every implementer has to read, decide about, and either build or explicitly skip. So the bar is deliberately higher than for a doc change.

A proposal needs four things:

1. **The problem**, stated as a case the current format cannot express. Concrete, with the profile you tried to write.
2. **The proposed change**, as a schema diff.
3. **Interop evidence**: at least two harnesses where this concept already exists, or a clear argument for why it should exist in all of them.
4. **An implementer**: someone willing to build it. A field nobody implements is worse than no field, because it makes conformance statements less meaningful.

Changes that would widen what an agent can do to itself, or that would let a profile grant capability, will be declined regardless of how they are argued. Those boundaries are load-bearing. See [security.md](spec/v1/security.md).

## Versioning

Within major version 1, a change may not:

- add a required field
- remove a field
- change the meaning of an existing field
- change a default

Anything in that list waits for 2.0. Additive optional fields go in a minor version.

`metadata.annotations` exists precisely so that harness-specific data does not need a spec change. Namespace your keys and use it.

## Development

```bash
git clone https://github.com/alexmerced-oss/open-agent-profile
cd open-agent-profile
pip install -e ".[dev]"
pytest
```

The suite must be green before a pull request, and new behavior needs a test. For anything touching the applicator or the security boundaries, add the negative test first: the one that proves the thing that must not happen does not happen.

```bash
oap-validate examples/ tests/deltas/ --strict
oap-validate examples/invalid/ --expect-invalid
```

## Writing style for docs and spec text

- Say what a rule is, then why it exists. The why is what stops someone from "simplifying" it later.
- Prefer a concrete failure to an abstract warning. "A dict merge grants privilege" beats "be careful with merges".
- No em dashes.
- Do not pad. If a section is short because the topic is small, leave it short.

## Code of conduct

Be straightforward and assume competence. Disagree about the technical substance, not about the person.
