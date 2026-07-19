#!/usr/bin/env python3
"""Enforce the advisory-ignore review policy.

Single source of truth: ``scripts/advisories.json``. Every suppressed RUSTSEC
advisory carries a ``review_by`` date. This script FAILS (exit 1) if today's
date is on or past any advisory's ``review_by`` date, so CI stops silently
ignoring an advisory whose review window has lapsed.

It also verifies that ``audit.toml`` and ``deny.toml`` list exactly the same
advisory IDs as the shared JSON, so the three files cannot drift apart.

Usage:
    python3 scripts/check-advisories.py
"""

from __future__ import annotations

import datetime as dt
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
ADVISORIES = ROOT / "scripts" / "advisories.json"
AUDIT_TOML = ROOT / "audit.toml"
DENY_TOML = ROOT / "deny.toml"

RUSTSEC_RE = re.compile(r"RUSTSEC-\d{4}-\d{4}")


def load_advisories() -> list[dict]:
    data = json.loads(ADVISORIES.read_text(encoding="utf-8"))
    return data["advisories"]


def ids_in_file(path: pathlib.Path) -> set[str]:
    """Collect every RUSTSEC id that appears inside an ``ignore`` block."""
    text = path.read_text(encoding="utf-8")
    return set(RUSTSEC_RE.findall(text))


def main() -> int:
    today = dt.date.today()
    advisories = load_advisories()
    shared_ids = {a["id"] for a in advisories}

    failures: list[str] = []

    # 1. Review-date enforcement.
    for adv in advisories:
        review_by = dt.date.fromisoformat(adv["review_by"])
        if today >= review_by:
            failures.append(
                f"{adv['id']}: review_by {adv['review_by']} has lapsed "
                f"(today is {today}). Re-triage this advisory or bump review_by."
            )
        else:
            print(f"OK  {adv['id']}: review_by {adv['review_by']} (in the future)")

    # 2. Drift enforcement — audit.toml and deny.toml must match the shared set.
    for path in (AUDIT_TOML, DENY_TOML):
        found = ids_in_file(path)
        missing = shared_ids - found
        extra = found - shared_ids
        if missing:
            failures.append(f"{path.name} is missing advisories: {sorted(missing)}")
        if extra:
            failures.append(f"{path.name} has stale advisories not in advisories.json: {sorted(extra)}")

    if failures:
        print("\nadvisory policy check FAILED:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(f"\nadvisory policy check PASSED ({len(advisories)} advisories, all review dates in the future).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
