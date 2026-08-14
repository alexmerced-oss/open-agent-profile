#!/usr/bin/env python3
"""Reference applicator for Open Agent Profile v1 state deltas.

Implements SPEC section 5.6 end to end: validate, revision check, scope check,
atomic apply, retention, revision bump, history append, atomic file write. This
is the piece most implementations get subtly wrong, so it exists here as an
executable reference rather than as prose.

What it deliberately does NOT do: apply `proposals`. Those go to a human. There
is no flag to change that, by design (SPEC 6.4, L2-A7).

Usage:
    oap-apply profile.agent.yaml session.delta.yaml
    oap-apply profile.agent.yaml session.delta.yaml --dry-run
    oap-apply profile.agent.yaml session.delta.yaml --approve
"""

from __future__ import annotations

import argparse
import copy
import json
import os
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import yaml

from oap.validate import (
    load_document,
    load_schema,
    profile_digest,
    validate_file,
    yaml_load,
)

STATE_COLLECTIONS = ("facts", "preferences", "glossary", "open_threads")


class ApplyError(RuntimeError):
    """A delta cannot be applied. The profile on disk is unchanged."""


class Conflict(ApplyError):
    """Revision mismatch. Another writer got there first (SPEC 6.3)."""


def now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


# --------------------------------------------------------------------------
# JSON Pointer, RFC 6901, plus the id: convenience form (L2-A14)
# --------------------------------------------------------------------------


def unescape(token: str) -> str:
    return token.replace("~1", "/").replace("~0", "~")


def resolve_tokens(doc: dict[str, Any], pointer: str) -> list[str]:
    """Split a pointer and rewrite `id:<entry-id>` tokens into array indices."""
    if pointer == "":
        return []
    if not pointer.startswith("/"):
        raise ApplyError(f"pointer must start with '/': {pointer!r}")

    raw = [unescape(part) for part in pointer.split("/")[1:]]
    resolved: list[str] = []
    cursor: Any = doc

    for token in raw:
        if isinstance(cursor, list) and token.startswith("id:"):
            wanted = token[3:]
            index = next(
                (i for i, item in enumerate(cursor) if isinstance(item, dict) and item.get("id") == wanted),
                None,
            )
            if index is None:
                resolved.append("-missing-")
                cursor = None
                continue
            resolved.append(str(index))
            cursor = cursor[index]
            continue

        resolved.append(token)
        if token == "-":
            cursor = None
        elif isinstance(cursor, dict):
            cursor = cursor.get(token)
        elif isinstance(cursor, list) and token.isdigit() and int(token) < len(cursor):
            cursor = cursor[int(token)]
        else:
            cursor = None

    return resolved


def apply_operation(doc: dict[str, Any], op: dict[str, Any], warnings: list[str]) -> None:
    kind = op["op"]
    pointer = op["path"]

    if not (pointer == "/state" or pointer.startswith("/state/")):
        # L2-A2. The escalation boundary. Never relax this.
        raise ApplyError(f"operation path {pointer!r} is outside /state")

    tokens = resolve_tokens(doc, pointer)
    if not tokens:
        raise ApplyError("operation path must not be the document root")

    if "-missing-" in tokens:
        if kind == "remove":
            warnings.append(f"remove on missing path {pointer!r} ignored")  # L2-A13
            return
        raise ApplyError(f"path {pointer!r} does not resolve; no entry with that id")

    parent: Any = doc
    for token in tokens[:-1]:
        if isinstance(parent, dict):
            parent = parent.setdefault(token, {})
        elif isinstance(parent, list):
            if not token.isdigit() or int(token) >= len(parent):
                raise ApplyError(f"path {pointer!r} does not resolve")
            parent = parent[int(token)]
        else:
            raise ApplyError(f"path {pointer!r} does not resolve")

    last = tokens[-1]

    if isinstance(parent, list):
        if kind == "add":
            if last == "-":
                parent.append(op["value"])
            elif last.isdigit():
                parent.insert(int(last), op["value"])
            else:
                raise ApplyError(f"cannot add at {pointer!r}: array index expected")
        elif kind == "replace":
            if not last.isdigit() or int(last) >= len(parent):
                raise ApplyError(f"cannot replace at {pointer!r}: index out of range")
            parent[int(last)] = op["value"]
        elif kind == "remove":
            if not last.isdigit() or int(last) >= len(parent):
                warnings.append(f"remove on missing path {pointer!r} ignored")
                return
            parent.pop(int(last))
    elif isinstance(parent, dict):
        if kind in ("add", "replace"):
            parent[last] = op["value"]
        elif kind == "remove":
            if last not in parent:
                warnings.append(f"remove on missing path {pointer!r} ignored")
                return
            parent.pop(last)
    else:
        raise ApplyError(f"path {pointer!r} does not resolve to a container")


# --------------------------------------------------------------------------
# Retention (L2-A9)
# --------------------------------------------------------------------------


def sort_key(entry: dict[str, Any], strategy: str) -> Any:
    if strategy == "least_confident":
        return entry.get("confidence", 1.0)
    if strategy == "oldest":
        return entry.get("learned_at") or entry.get("opened_at") or ""
    return entry.get("last_used_at") or entry.get("updated_at") or entry.get("learned_at") or ""


def enforce_retention(doc: dict[str, Any], warnings: list[str]) -> None:
    retention = doc.get("spec", {}).get("lifecycle", {}).get("retention") or {}
    state = doc.get("state") or {}
    strategy = retention.get("eviction", "least_recently_used")
    today = now()

    ttl_days = retention.get("fact_ttl_days")
    for collection in ("facts", "preferences"):
        entries = state.get(collection)
        if not isinstance(entries, list):
            continue

        if ttl_days:
            kept = []
            for entry in entries:
                expires = entry.get("expires_at")
                if expires and expires < today and not entry.get("pinned"):
                    warnings.append(f"evicted expired {collection} entry {entry.get('id')!r}")
                    continue
                kept.append(entry)
            state[collection] = kept
            entries = kept

        cap = retention.get("max_facts") if collection == "facts" else None
        if cap is not None and len(entries) > cap:
            pinned = [e for e in entries if e.get("pinned")]
            rest = sorted((e for e in entries if not e.get("pinned")), key=lambda e: sort_key(e, strategy))
            room = max(cap - len(pinned), 0)
            evicted = rest[: max(len(rest) - room, 0)]
            for entry in evicted:
                warnings.append(f"evicted {collection} entry {entry.get('id')!r} ({strategy})")
            keep_ids = {id(e) for e in pinned} | {id(e) for e in rest[len(evicted) :]}
            state[collection] = [e for e in entries if id(e) in keep_ids]

    cap = retention.get("max_open_threads")
    threads = state.get("open_threads")
    if cap is not None and isinstance(threads, list) and len(threads) > cap:
        active = [t for t in threads if t.get("status") in (None, "open", "blocked")]
        closed = [t for t in threads if t.get("status") in ("done", "abandoned")]
        closed.sort(key=lambda t: t.get("updated_at") or "")
        overflow = len(threads) - cap
        dropped = closed[:overflow]
        for thread in dropped:
            warnings.append(f"evicted closed thread {thread.get('id')!r}")
        remaining = [t for t in threads if t not in dropped]
        state["open_threads"] = remaining[-cap:] if len(remaining) > cap else remaining
        if len(remaining) > cap:
            warnings.append(f"open_threads still over cap after evicting closed threads ({len(active)} active)")

    max_history = retention.get("max_history", 50)
    history = doc.get("history")
    if isinstance(history, list) and len(history) > max_history:
        doc["history"] = history[len(history) - max_history :]


# --------------------------------------------------------------------------
# Atomic write (L2-A12)
# --------------------------------------------------------------------------


def dump(doc: dict[str, Any], path: Path) -> str:
    if path.name.endswith(".json"):
        return json.dumps(doc, indent=2, ensure_ascii=False) + "\n"
    return yaml.safe_dump(doc, sort_keys=False, allow_unicode=True, width=100)


def atomic_write(path: Path, text: str) -> None:
    directory = path.parent
    handle, temporary = tempfile.mkstemp(dir=directory, prefix=f".{path.name}.", suffix=".tmp")
    try:
        with os.fdopen(handle, "w", encoding="utf-8") as stream:
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        directory_fd = os.open(directory, os.O_RDONLY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    except BaseException:
        Path(temporary).unlink(missing_ok=True)
        raise


# --------------------------------------------------------------------------
# Apply
# --------------------------------------------------------------------------


def apply_delta(
    profile: dict[str, Any],
    delta: dict[str, Any],
    *,
    approved: bool = False,
    actor: str = "oap_apply",
) -> tuple[dict[str, Any], list[str], list[dict[str, Any]]]:
    """Return (new_profile, warnings, pending_proposals). Never mutates `profile`."""
    warnings: list[str] = []

    metadata = profile.get("metadata") or {}
    current = metadata.get("revision", 1)
    target = delta.get("target") or {}

    if target.get("name") != metadata.get("name"):
        raise ApplyError(f"delta targets {target.get('name')!r} but profile is {metadata.get('name')!r}")

    if target.get("revision") != current:
        # L2-A3. Rebase, queue, or reject. This reference implementation rejects
        # and reports both revisions; a harness may rebase id-addressed operations.
        raise Conflict(
            f"delta targets revision {target.get('revision')} but profile is at {current}; "
            "rebase, queue for review, or reject"
        )

    if (pinned := target.get("digest")) and pinned != profile_digest(profile):  # L2-A4
        raise ApplyError("target.digest does not match the profile on disk")

    writeback = (profile.get("spec", {}).get("lifecycle") or {}).get("writeback", "propose")
    if writeback == "off":
        raise ApplyError("lifecycle.writeback is 'off'; this profile does not accept deltas")
    if writeback == "propose" and not approved:
        raise ApplyError(
            "lifecycle.writeback is 'propose'; re-run with --approve to record human approval"
        )

    # L2-A5: work on a copy so a mid-sequence failure leaves the original intact.
    working = copy.deepcopy(profile)
    working.setdefault("state", {})

    for index, op in enumerate(delta.get("operations") or []):
        try:
            apply_operation(working, op, warnings)
        except ApplyError as exc:
            raise ApplyError(f"operation {index}: {exc}") from exc

    enforce_retention(working, warnings)

    stamp = now()
    working["metadata"]["revision"] = current + 1  # L2-A10
    working["metadata"]["updated_at"] = stamp
    if delta.get("operations"):
        working["state"]["updated_at"] = stamp
        working["state"]["revision"] = (working["state"].get("revision") or 0) + 1

    session = delta.get("session") or {}
    entry = {  # L2-A11
        "revision": working["metadata"]["revision"],
        "at": stamp,
        "by": session.get("id") or actor,
        "change": delta.get("summary") or f"{len(delta.get('operations') or [])} state operations",
        "sections": ["state"],
    }
    if session.get("id"):
        entry["session_id"] = session["id"]
    if session.get("harness"):
        entry["harness"] = session["harness"]
    if approved:
        entry["approved_by"] = actor
    working.setdefault("history", []).append(entry)

    enforce_retention(working, warnings)

    # L2-A7: proposals are returned for a human, never applied. No flag changes this.
    pending = list(delta.get("proposals") or [])
    for proposal in pending:
        path = proposal.get("path", "")
        if any(path.startswith(p) for p in ("/spec/tools", "/spec/permissions", "/spec/memory", "/spec/runtime/subagents")):
            proposal["risk"] = "high"  # L2-A8: computed here, not trusted from the document

    return working, warnings, pending


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Apply an OAP state delta to a profile.")
    parser.add_argument("profile", help="Path to the AgentProfile document.")
    parser.add_argument("delta", help="Path to the AgentStateDelta document.")
    parser.add_argument("--approve", action="store_true", help="Record human approval, required when writeback is 'propose'.")
    parser.add_argument("--dry-run", action="store_true", help="Print the result without writing.")
    parser.add_argument("--actor", default=os.environ.get("USER", "unknown"), help="Identity recorded in history.")
    args = parser.parse_args(argv)

    profile_path = Path(args.profile)
    delta_path = Path(args.delta)

    profile_schema = load_schema("agent-profile.schema.json")
    delta_schema = load_schema("agent-state-delta.schema.json")

    for path in (profile_path, delta_path):
        report = validate_file(path, profile_schema, delta_schema)
        if not report.ok:  # L2-A1
            print(f"invalid document: {path}", file=sys.stderr)
            for error in report.errors:
                print(f"  error: {error}", file=sys.stderr)
            return 1

    profile, _ = load_document(profile_path)
    delta, _ = load_document(delta_path)

    try:
        updated, warnings, pending = apply_delta(profile, delta, approved=args.approve, actor=args.actor)
    except Conflict as exc:
        print(f"conflict: {exc}", file=sys.stderr)
        return 3
    except ApplyError as exc:
        print(f"cannot apply: {exc}", file=sys.stderr)
        return 1

    for warning in warnings:
        print(f"warn: {warning}", file=sys.stderr)

    if pending:
        print(f"\n{len(pending)} proposal(s) require human review and were NOT applied:", file=sys.stderr)
        for proposal in pending:
            print(f"  [{proposal.get('risk', 'unknown')}] {proposal['path']}", file=sys.stderr)
            print(f"      value:     {json.dumps(proposal.get('value'))}", file=sys.stderr)
            print(f"      rationale: {proposal['rationale']}", file=sys.stderr)

    text = dump(updated, profile_path)
    if args.dry_run:
        print(text)
        print(f"(dry run) would write revision {updated['metadata']['revision']}", file=sys.stderr)
        return 0

    atomic_write(profile_path, text)
    print(f"wrote {profile_path} at revision {updated['metadata']['revision']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
