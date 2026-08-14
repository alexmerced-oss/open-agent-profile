#!/usr/bin/env python3
"""Build an AgentStateDelta from what a session learned.

Assigns stable content-derived ids so re-learning a fact updates it in place
rather than duplicating it, stamps provenance, and routes capability requests
into `proposals` where a human approves them.

Usage:
    python oap_delta.py PROFILE \
        --session-id sess_123 --harness claude-code \
        --summary "What happened." \
        --fact "text|source|confidence" \
        --preference "text|source|confidence" \
        --thread "id|status|detail" \
        --request-tool shell "Why it was needed." \
        > session.delta.yaml
"""

from __future__ import annotations

import argparse
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

try:
    import yaml

    from oap.validate import load_document
except ImportError:  # pragma: no cover
    sys.exit("this script requires the reference tools: pip install open-agent-profile")


def now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def stable_id(text: str, prefix: str) -> str:
    """Content-derived, so the same learned fact yields the same id twice.

    This is what makes `add` idempotent across sessions, and what lets a
    conflicting delta be rebased instead of rejected.
    """
    slug = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")[:48].rstrip("-")
    return f"{prefix}-{slug}" or f"{prefix}-entry"


def parse_entry(raw: str, prefix: str, stamp: str) -> dict:
    parts = [part.strip() for part in raw.split("|")]
    text = parts[0]
    if not text:
        raise SystemExit(f"empty {prefix} text in {raw!r}")

    entry = {"id": stable_id(text, prefix), "text": text, "learned_at": stamp}
    if len(parts) > 1 and parts[1]:
        entry["source"] = parts[1]
    if len(parts) > 2 and parts[2]:
        try:
            entry["confidence"] = float(parts[2])
        except ValueError:
            raise SystemExit(f"confidence must be a number between 0 and 1: {parts[2]!r}") from None
    return entry


def parse_thread(raw: str, stamp: str) -> dict:
    parts = [part.strip() for part in raw.split("|")]
    if len(parts) < 2:
        raise SystemExit(f"--thread needs at least 'id|status': {raw!r}")

    thread = {"id": parts[0], "title": parts[0].replace("thread-", "").replace("-", " "), "status": parts[1]}
    if len(parts) > 2 and parts[2]:
        thread["detail"] = parts[2]
    if len(parts) > 3 and parts[3]:
        thread["title"] = parts[3]
    thread["updated_at"] = stamp
    return thread


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Build an OAP state delta from session evidence.")
    parser.add_argument("profile", help="Profile the session ran from.")
    parser.add_argument("--session-id", required=True)
    parser.add_argument("--harness", default="")
    parser.add_argument("--model-used", default="")
    parser.add_argument("--summary", default="")
    parser.add_argument("--state-summary", default="", help="Replace the profile's state.summary self-briefing.")
    parser.add_argument("--fact", action="append", default=[], metavar="TEXT|SOURCE|CONFIDENCE")
    parser.add_argument("--preference", action="append", default=[], metavar="TEXT|SOURCE|CONFIDENCE")
    parser.add_argument("--thread", action="append", default=[], metavar="ID|STATUS|DETAIL|TITLE")
    parser.add_argument("--forget", action="append", default=[], metavar="COLLECTION/ID", help="e.g. facts/fact-old")
    parser.add_argument(
        "--request-tool",
        action="append",
        nargs=2,
        default=[],
        metavar=("TOOL", "RATIONALE"),
        help="Ask for a tool the profile does not grant. Becomes a proposal, never an operation.",
    )
    args = parser.parse_args(argv)

    profile_path = Path(args.profile)
    try:
        profile, _ = load_document(profile_path)
    except Exception as exc:  # noqa: BLE001
        return _fail(f"cannot read profile {profile_path}: {exc}")

    metadata = profile.get("metadata") or {}
    writeback = (profile.get("spec", {}).get("lifecycle") or {}).get("writeback", "propose")
    if writeback == "off":
        return _fail(f"{metadata.get('name')} has lifecycle.writeback: off; this profile does not accept deltas")

    stamp = now()
    existing = profile.get("state") or {}
    operations: list[dict] = []

    for collection, values, prefix in (
        ("facts", args.fact, "fact"),
        ("preferences", args.preference, "pref"),
    ):
        known = {entry.get("id") for entry in existing.get(collection) or []}
        for raw in values:
            entry = parse_entry(raw, prefix, stamp)
            if entry["id"] in known:
                # Idempotent update rather than a duplicate.
                operations.append(
                    {"op": "replace", "path": f"/state/{collection}/id:{entry['id']}", "value": entry}
                )
            else:
                operations.append({"op": "add", "path": f"/state/{collection}/-", "value": entry})

    known_threads = {thread.get("id"): thread for thread in existing.get("open_threads") or []}
    for raw in args.thread:
        thread = parse_thread(raw, stamp)
        if previous := known_threads.get(thread["id"]):
            merged = {**previous, **thread}
            operations.append({"op": "replace", "path": f"/state/open_threads/id:{thread['id']}", "value": merged})
        else:
            thread.setdefault("opened_at", stamp)
            operations.append({"op": "add", "path": "/state/open_threads/-", "value": thread})

    for target in args.forget:
        if "/" not in target:
            return _fail(f"--forget expects COLLECTION/ID, got {target!r}")
        collection, entry_id = target.split("/", 1)
        operations.append({"op": "remove", "path": f"/state/{collection}/id:{entry_id}"})

    if args.state_summary:
        operations.append({"op": "replace", "path": "/state/summary", "value": args.state_summary})

    proposals = []
    if args.request_tool:
        allow = list((profile.get("spec", {}).get("tools") or {}).get("allow") or [])
        for tool, rationale in args.request_tool:
            if tool in allow:
                print(f"note: {tool!r} is already granted; skipping the request", file=sys.stderr)
                continue
            allow.append(tool)
        if allow != list((profile.get("spec", {}).get("tools") or {}).get("allow") or []):
            proposals.append(
                {
                    "path": "/spec/tools/allow",
                    "op": "replace",
                    "value": allow,
                    "rationale": " ".join(rationale for _, rationale in args.request_tool),
                    "risk": "high",
                }
            )

    if not operations and not proposals:
        return _fail("nothing to write; an empty delta burns a revision and teaches the reviewer to skim")

    delta = {
        "oap": "1.0",
        "kind": "AgentStateDelta",
        "target": {"name": metadata.get("name"), "revision": metadata.get("revision", 1)},
        "session": {"id": args.session_id, "ended_at": stamp, "outcome": "completed"},
    }
    if entry_id := metadata.get("id"):
        delta["target"]["id"] = entry_id
    if args.harness:
        delta["session"]["harness"] = args.harness
    if args.model_used:
        delta["session"]["model_used"] = args.model_used
    if args.summary:
        delta["summary"] = args.summary
    if operations:
        delta["operations"] = operations
    if proposals:
        delta["proposals"] = proposals

    print(yaml.safe_dump(delta, sort_keys=False, allow_unicode=True, width=100))

    print(
        f"\n{len(operations)} operation(s), {len(proposals)} proposal(s) against "
        f"{metadata.get('name')} revision {metadata.get('revision', 1)}.\n"
        "Review every value for secrets before applying.",
        file=sys.stderr,
    )
    return 0


def _fail(message: str) -> int:
    print(message, file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
