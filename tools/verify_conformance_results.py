#!/usr/bin/env python3
"""Verify checked-in OAP conformance claims and their claimed level surfaces."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

from jsonschema import Draft202012Validator, FormatChecker

ROOT = Path(__file__).resolve().parent.parent
SCHEMA = json.loads((ROOT / "conformance" / "result.schema.json").read_text(encoding="utf-8"))
LEVEL_1 = {
    *(f"L1-P{i}" for i in range(1, 7)),
    *(f"L1-D{i}" for i in range(1, 5)),
    *(f"L1-I{i}" for i in range(1, 13)),
    "L1-R1", "L1-R2",
}
LEVEL_2 = LEVEL_1 | {
    *(f"L2-S{i}" for i in range(1, 6)),
    *(f"L2-G{i}" for i in range(1, 5)),
    *(f"L2-A{i}" for i in range(1, 14)),
}
LEVEL_3 = LEVEL_2 | {
    "L3-E1", "L3-E2", "L3-M1", "L3-K1", "L3-Y1",
    "L3-B1", "L3-B2", "L3-G1", "L3-G2", "L3-G3",
}
BY_LEVEL = {1: LEVEL_1, 2: LEVEL_2, 3: LEVEL_3}


def main() -> int:
    failures: list[str] = []
    validator = Draft202012Validator(SCHEMA, format_checker=FormatChecker())
    paths = sorted((ROOT / "conformance" / "results").glob("*.json"))
    if not paths:
        failures.append("no conformance results found")
    for path in paths:
        result = json.loads(path.read_text(encoding="utf-8"))
        for error in validator.iter_errors(result):
            failures.append(f"{path.name}: schema: {error.message}")
        if result.get("failed"):
            failures.append(f"{path.name}: failed checks are not empty")
        revision = result.get("fixture_revision", "")
        if not re.fullmatch(r"[0-9a-f]{40}", revision):
            failures.append(f"{path.name}: fixture_revision is not a full Git commit")
        level = result.get("level")
        if level in BY_LEVEL:
            missing = BY_LEVEL[level] - set(result.get("passed", []))
            if missing:
                failures.append(f"{path.name}: missing Level {level} evidence: {sorted(missing)}")
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"Verified {len(paths)} OAP conformance result(s).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
