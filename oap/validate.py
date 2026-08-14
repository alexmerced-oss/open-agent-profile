#!/usr/bin/env python3
"""Reference validator for Open Agent Profile v1 documents.

Validates AgentProfile and AgentStateDelta documents against the normative JSON
Schemas, then applies the structural rules that JSON Schema alone cannot express:
literal secret detection, workspace escape, Markdown encoding rules, and version
gating.

This validates *documents*. It cannot validate *harness behavior*. Requirements
L1-I3 (narrowing), L2-S3 (state is not authority), L2-A5 (atomic apply), and
L2-A7 (proposal gate) can only be demonstrated by an implementation's own tests.
See spec/v1/conformance.md section 5.

Usage:
    oap-validate examples/
    oap-validate my-agent.agent.yaml --strict
    oap-validate examples/invalid/ --expect-invalid
    oap-validate code-reviewer.agent.yaml --digest
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

try:
    import yaml
except ImportError:  # pragma: no cover
    sys.exit("oap_validate requires PyYAML: pip install pyyaml")

try:
    from jsonschema import Draft202012Validator
except ImportError:  # pragma: no cover
    sys.exit("oap_validate requires jsonschema: pip install jsonschema")


SPEC_MAJOR = 1


def _schema_dir() -> Path:
    """Locate the normative schemas in both a source checkout and an installed wheel."""
    here = Path(__file__).resolve().parent
    for candidate in (here / "schema" / "v1", here.parent / "schema" / "v1"):
        if candidate.is_dir():
            return candidate
    raise RuntimeError("cannot locate OAP schemas; expected schema/v1 beside or above the oap package")


SCHEMA_DIR = _schema_dir()
PROFILE_SUFFIXES = (".agent.yaml", ".agent.yml", ".agent.json", ".agent.md")
DELTA_SUFFIXES = (".delta.yaml", ".delta.yml", ".delta.json")

# Credential shapes worth refusing outright. Deliberately narrow: a noisy secret
# scanner that people disable protects nothing.
SECRET_PATTERNS = [
    (re.compile(r"\bsk-[A-Za-z0-9_-]{16,}"), "OpenAI-style API key"),
    (re.compile(r"\bsk-ant-[A-Za-z0-9_-]{16,}"), "Anthropic API key"),
    (re.compile(r"\bgh[pousr]_[A-Za-z0-9]{20,}"), "GitHub token"),
    (re.compile(r"\bAKIA[0-9A-Z]{16}\b"), "AWS access key id"),
    (re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{10,}"), "Slack token"),
    (re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH |PGP )?PRIVATE KEY-----"), "private key"),
    (re.compile(r"\bey[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}"), "JWT"),
]

ENV_REF = re.compile(r"^\$\{[A-Z][A-Z0-9_]{0,63}\}$")
HEADER_REF = re.compile(r"^(Bearer )?\$\{[A-Z][A-Z0-9_]{0,63}\}$")
VAR_REF = re.compile(r"\$\{\{\s*vars\.([A-Za-z_][A-Za-z0-9_]*)\s*\}\}")


class OAPLoader(yaml.SafeLoader):
    """YAML loader that keeps timestamps as strings.

    YAML 1.1 implicit typing turns an unquoted RFC 3339 timestamp into a native
    datetime. That silently breaks two things: JSON Schema `format: date-time`
    validation, which expects a string, and canonical-JSON digests, which are not
    computable from a datetime. Every OAP implementation reading YAML has to
    disable this resolver or it will produce digests that disagree with everyone
    else's. See docs/implementers-guide.md.
    """


OAPLoader.yaml_implicit_resolvers = {
    key: [(tag, regexp) for tag, regexp in resolvers if tag != "tag:yaml.org,2002:timestamp"]
    for key, resolvers in yaml.SafeLoader.yaml_implicit_resolvers.items()
}


def yaml_load(text: str) -> Any:
    return yaml.load(text, Loader=OAPLoader)  # noqa: S506 - OAPLoader derives from SafeLoader


@dataclass
class Report:
    path: Path
    kind: str = "unknown"
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    digests: dict[str, str] = field(default_factory=dict)

    @property
    def ok(self) -> bool:
        return not self.errors


# --------------------------------------------------------------------------
# Loading
# --------------------------------------------------------------------------


def load_schema(name: str) -> dict[str, Any]:
    return json.loads((SCHEMA_DIR / name).read_text(encoding="utf-8"))


def split_frontmatter(text: str) -> tuple[str, str]:
    """Return (frontmatter, body) for a Markdown-encoded profile."""
    if not text.startswith("---"):
        raise ValueError("Markdown encoding requires YAML frontmatter delimited by ---")
    end = text.find("\n---", 3)
    if end == -1:
        raise ValueError("unterminated YAML frontmatter")
    fm = text[3:end].lstrip("\n")
    body = text[end + 4 :].lstrip("\n")
    return fm, body


def load_document(path: Path) -> tuple[dict[str, Any], list[str]]:
    """Load a document from any supported encoding. Returns (doc, warnings)."""
    warnings: list[str] = []
    text = path.read_text(encoding="utf-8")

    if path.name.endswith(".md"):
        fm, body = split_frontmatter(text)
        doc = yaml_load(fm) or {}
        if not isinstance(doc, dict):
            raise ValueError("frontmatter must be a mapping")
        role = doc.setdefault("spec", {}).setdefault("role", {})
        if not isinstance(role, dict):
            raise ValueError("spec.role must be a mapping")
        if "instructions" in role:
            # SPEC 2.1 / L3-G1
            raise ValueError(
                "Markdown encoding supplies spec.role.instructions in both "
                "frontmatter and body; the body is authoritative and frontmatter "
                "must omit it"
            )
        if not body.strip():
            raise ValueError("Markdown encoding requires a non-empty body")
        role["instructions"] = body.rstrip() + "\n"
        return doc, warnings

    if path.name.endswith(".json"):
        return json.loads(text), warnings

    doc = yaml_load(text)
    if not isinstance(doc, dict):
        raise ValueError("document must be a mapping")
    return doc, warnings


# --------------------------------------------------------------------------
# Digests (SPEC 2.2)
# --------------------------------------------------------------------------


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def profile_digest(doc: dict[str, Any]) -> str:
    return "sha256:" + hashlib.sha256(canonical_json(doc)).hexdigest()


def spec_digest(doc: dict[str, Any]) -> str:
    subset = {"metadata": doc.get("metadata"), "spec": doc.get("spec")}
    return "sha256:" + hashlib.sha256(canonical_json(subset)).hexdigest()


# --------------------------------------------------------------------------
# Structural rules beyond the schema
# --------------------------------------------------------------------------


def walk_strings(value: Any, pointer: str = "") -> Iterable[tuple[str, str]]:
    if isinstance(value, str):
        yield pointer, value
    elif isinstance(value, dict):
        for key, sub in value.items():
            token = str(key).replace("~", "~0").replace("/", "~1")
            yield from walk_strings(sub, f"{pointer}/{token}")
    elif isinstance(value, list):
        for index, sub in enumerate(value):
            yield from walk_strings(sub, f"{pointer}/{index}")


def check_version(doc: dict[str, Any], report: Report) -> bool:
    raw = doc.get("oap")
    if not isinstance(raw, str) or "." not in raw:
        report.errors.append("oap: missing or malformed spec version string")
        return False
    major = raw.split(".", 1)[0]
    if not major.isdigit() or int(major) != SPEC_MAJOR:
        # L1-P3
        report.errors.append(f"oap: major version {major} is not supported by this validator (expected {SPEC_MAJOR})")
        return False
    minor = raw.split(".", 1)[1]
    if minor.isdigit() and int(minor) > 0:
        report.warnings.append(f"oap: minor version {raw} is newer than 1.0; unrecognized fields will be ignored")
    return True


def check_secrets(doc: dict[str, Any], report: Report) -> None:
    """SPEC 7.3. Literal credentials anywhere in the document."""
    for pointer, text in walk_strings(doc):
        for pattern, label in SECRET_PATTERNS:
            if pattern.search(text):
                report.errors.append(f"{pointer}: looks like a literal {label}; use a ${{VARIABLE}} reference")
                break


def check_mcp_refs(doc: dict[str, Any], report: Report) -> None:
    """SPEC 3.4 / L1-I6. env and header values must be environment references."""
    servers = doc.get("spec", {}).get("tools", {}).get("mcp_servers") or []
    for index, server in enumerate(servers):
        if not isinstance(server, dict):
            continue
        base = f"/spec/tools/mcp_servers/{index}"
        for key, value in (server.get("env") or {}).items():
            if not isinstance(value, str) or not ENV_REF.match(value):
                report.errors.append(f"{base}/env/{key}: must be a same-name ${{VARIABLE}} reference, not a literal")
        for key, value in (server.get("headers") or {}).items():
            if not isinstance(value, str) or not HEADER_REF.match(value):
                report.errors.append(f"{base}/headers/{key}: must be '${{VARIABLE}}' or 'Bearer ${{VARIABLE}}'")


def escapes_workspace(raw: str) -> bool:
    if raw.startswith("/") or re.match(r"^[A-Za-z]:[\\/]", raw) or raw.startswith("\\\\"):
        return True
    depth = 0
    for part in re.split(r"[\\/]+", raw):
        if part in ("", "."):
            continue
        if part == "..":
            depth -= 1
            if depth < 0:
                return True
        else:
            depth += 1
    return False


def check_paths(doc: dict[str, Any], report: Report) -> None:
    """SPEC 7.6 / L1-I5. Nothing may resolve outside the workspace."""
    spec = doc.get("spec", {})
    targets: list[tuple[str, str]] = []

    for index, entry in enumerate(spec.get("context", {}).get("files") or []):
        if isinstance(entry, dict) and isinstance(entry.get("path"), str):
            targets.append((f"/spec/context/files/{index}/path", entry["path"]))

    cwd = spec.get("context", {}).get("working_directory")
    if isinstance(cwd, str):
        targets.append(("/spec/context/working_directory", cwd))

    fs = spec.get("permissions", {}).get("filesystem") or {}
    for key in ("read_roots", "write_roots", "deny_paths"):
        for index, raw in enumerate(fs.get(key) or []):
            if isinstance(raw, str):
                targets.append((f"/spec/permissions/filesystem/{key}/{index}", raw))

    for pointer, raw in targets:
        if escapes_workspace(raw):
            report.errors.append(f"{pointer}: {raw!r} resolves outside the workspace")


def check_variables(doc: dict[str, Any], report: Report) -> None:
    """SPEC 3.6. Undefined substitution variables are an error, not an empty string."""
    spec = doc.get("spec", {})
    declared = set((spec.get("context", {}).get("variables") or {}).keys())
    role = spec.get("role", {})
    fields: list[tuple[str, str]] = []
    if isinstance(role.get("instructions"), str):
        fields.append(("/spec/role/instructions", role["instructions"]))
    for key in ("objectives", "constraints"):
        for index, item in enumerate(role.get(key) or []):
            if isinstance(item, str):
                fields.append((f"/spec/role/{key}/{index}", item))

    for pointer, text in fields:
        for name in VAR_REF.findall(text):
            if name not in declared:
                report.errors.append(f"{pointer}: references undefined variable {name!r}")

    # L2-S5: substitution never happens inside state, so a template there is a bug.
    for pointer, text in walk_strings(doc.get("state") or {}, "/state"):
        if VAR_REF.search(text):
            report.warnings.append(f"{pointer}: contains a ${{{{ vars.* }}}} template; substitution never runs inside state")


def check_state_ids(doc: dict[str, Any], report: Report) -> None:
    """Deltas address entries by id, so ids must be unique per collection."""
    state = doc.get("state") or {}
    for key in ("facts", "preferences", "open_threads"):
        seen: set[str] = set()
        for index, entry in enumerate(state.get(key) or []):
            if not isinstance(entry, dict):
                continue
            entry_id = entry.get("id")
            if entry_id in seen:
                report.errors.append(f"/state/{key}/{index}/id: duplicate id {entry_id!r}")
            seen.add(entry_id)


def check_metadata_hygiene(doc: dict[str, Any], path: Path, report: Report) -> None:
    metadata = doc.get("metadata") or {}
    name = metadata.get("name")

    stem = path.name
    for suffix in PROFILE_SUFFIXES:
        if stem.endswith(suffix):
            stem = stem[: -len(suffix)]
            break
    if name and stem and name != stem:
        # L1-D4
        report.warnings.append(f"metadata.name {name!r} does not match file name {stem!r}; metadata.name wins")

    if "trust" in metadata:
        # SPEC 7.1: assigned by the resolver, never read from the file.
        report.warnings.append("metadata.trust is present in the file; resolvers must discard and recompute it")

    description = metadata.get("description") or ""
    if description and len(description.split()) < 4:
        report.warnings.append(
            "metadata.description is very short; harnesses route on it, so it should say when to use this agent"
        )


def check_history(doc: dict[str, Any], report: Report) -> None:
    history = doc.get("history") or []
    revisions = [entry.get("revision") for entry in history if isinstance(entry, dict)]
    if revisions != sorted(r for r in revisions if isinstance(r, int)):
        report.errors.append("/history: entries must be ordered oldest first by revision")
    current = (doc.get("metadata") or {}).get("revision")
    if isinstance(current, int) and revisions:
        newest = revisions[-1]
        if isinstance(newest, int) and newest > current:
            report.errors.append(f"/history: newest entry revision {newest} exceeds metadata.revision {current}")


def check_tools_policy(doc: dict[str, Any], report: Report) -> None:
    tools = doc.get("spec", {}).get("tools") or {}
    policy = tools.get("policy", "allowlist")
    if policy == "inherit" and (tools.get("allow") or tools.get("deny")):
        report.warnings.append("/spec/tools: policy is 'inherit', so allow and deny are ignored")
    if policy == "allowlist" and not tools.get("allow") and tools:
        report.warnings.append("/spec/tools: policy is 'allowlist' with an empty allow list, so the agent gets no tools")

    granted = set(tools.get("allow") or [])
    for index, binding in enumerate(tools.get("bindings") or []):
        if isinstance(binding, dict) and policy == "allowlist" and granted and binding.get("name") not in granted:
            report.warnings.append(
                f"/spec/tools/bindings/{index}: binding for {binding.get('name')!r} is inert; the tool is not in allow"
            )


def check_delta(doc: dict[str, Any], report: Report) -> None:
    """SPEC 6. Scope, pointer shape, and the proposals boundary."""
    for index, op in enumerate(doc.get("operations") or []):
        if not isinstance(op, dict):
            continue
        pointer = op.get("path", "")
        if not isinstance(pointer, str) or not (pointer == "/state" or pointer.startswith("/state/")):
            # L2-A2, the escalation boundary
            report.errors.append(
                f"/operations/{index}/path: {pointer!r} is outside /state; changes to metadata or spec "
                "belong in `proposals`, where a human approves them"
            )

    for index, proposal in enumerate(doc.get("proposals") or []):
        if not isinstance(proposal, dict):
            continue
        pointer = proposal.get("path", "")
        widening = ("/spec/tools", "/spec/permissions", "/spec/memory", "/spec/runtime/subagents")
        if isinstance(pointer, str) and any(pointer.startswith(prefix) for prefix in widening):
            declared = proposal.get("risk")
            if declared != "high":
                # L2-A8: applicators compute risk themselves; a document claiming
                # otherwise is exactly what an applicator must not believe.
                report.warnings.append(
                    f"/proposals/{index}: touches {pointer} and must be treated as high risk "
                    f"regardless of the declared risk {declared!r}"
                )


# --------------------------------------------------------------------------
# Driver
# --------------------------------------------------------------------------


def validate_file(path: Path, profile_schema: dict[str, Any], delta_schema: dict[str, Any]) -> Report:
    report = Report(path=path)

    try:
        doc, warnings = load_document(path)
        report.warnings.extend(warnings)
    except Exception as exc:  # noqa: BLE001 - report, do not crash the run
        report.errors.append(f"parse error: {exc}")
        return report

    kind = doc.get("kind")
    report.kind = str(kind)

    if not check_version(doc, report):
        return report

    if kind == "AgentProfile":
        schema = profile_schema
    elif kind == "AgentStateDelta":
        schema = delta_schema
    else:
        # L1-P5
        report.errors.append(f"kind: {kind!r} is not a known 1.x kind; unknown kinds must be rejected, not guessed at")
        return report

    for error in sorted(Draft202012Validator(schema).iter_errors(doc), key=lambda e: list(e.absolute_path)):
        pointer = "/" + "/".join(str(part) for part in error.absolute_path)
        report.errors.append(f"{pointer if pointer != '/' else '<root>'}: {error.message}")

    check_secrets(doc, report)

    if kind == "AgentProfile":
        check_mcp_refs(doc, report)
        check_paths(doc, report)
        check_variables(doc, report)
        check_state_ids(doc, report)
        check_metadata_hygiene(doc, path, report)
        check_history(doc, report)
        check_tools_policy(doc, report)
        report.digests = {"profile": profile_digest(doc), "spec": spec_digest(doc)}
    else:
        check_delta(doc, report)

    return report


def collect(paths: list[str]) -> list[Path]:
    found: list[Path] = []
    suffixes = PROFILE_SUFFIXES + DELTA_SUFFIXES
    for raw in paths:
        path = Path(raw)
        if path.is_dir():
            found.extend(sorted(p for p in path.rglob("*") if p.is_file() and p.name.endswith(suffixes)))
        elif path.is_file():
            found.append(path)
        else:
            print(f"warning: {raw} does not exist", file=sys.stderr)
    return found


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Validate Open Agent Profile v1 documents.")
    parser.add_argument("paths", nargs="+", help="Files or directories to validate.")
    parser.add_argument("--strict", action="store_true", help="Treat warnings as errors.")
    parser.add_argument("--json", action="store_true", help="Emit machine-readable results.")
    parser.add_argument("--digest", action="store_true", help="Print profile and spec digests.")
    parser.add_argument(
        "--expect-invalid",
        action="store_true",
        help="Invert the exit status: succeed only if every document is rejected. For negative fixtures.",
    )
    args = parser.parse_args(argv)

    profile_schema = load_schema("agent-profile.schema.json")
    delta_schema = load_schema("agent-state-delta.schema.json")

    files = collect(args.paths)
    if not files:
        print("no documents found", file=sys.stderr)
        return 2

    reports = [validate_file(path, profile_schema, delta_schema) for path in files]
    for report in reports:
        if args.strict:
            report.errors.extend(f"(strict) {w}" for w in report.warnings)
            report.warnings = []

    if args.json:
        print(
            json.dumps(
                [
                    {
                        "path": str(r.path),
                        "kind": r.kind,
                        "ok": r.ok,
                        "errors": r.errors,
                        "warnings": r.warnings,
                        "digests": r.digests,
                    }
                    for r in reports
                ],
                indent=2,
            )
        )
    else:
        for report in reports:
            status = "PASS" if report.ok else "FAIL"
            print(f"{status}  {report.path}  ({report.kind})")
            for error in report.errors:
                print(f"        error: {error}")
            for warning in report.warnings:
                print(f"        warn:  {warning}")
            if args.digest and report.digests:
                print(f"        profile digest: {report.digests['profile']}")
                print(f"        spec digest:    {report.digests['spec']}")

        passed = sum(1 for r in reports if r.ok)
        print(f"\n{passed}/{len(reports)} documents valid")

    if args.expect_invalid:
        unexpected = [r for r in reports if r.ok]
        for report in unexpected:
            print(f"unexpected pass: {report.path}", file=sys.stderr)
        return 1 if unexpected else 0

    return 0 if all(r.ok for r in reports) else 1


if __name__ == "__main__":
    raise SystemExit(main())
