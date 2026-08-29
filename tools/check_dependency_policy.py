#!/usr/bin/env python3
"""Fail when Cargo.lock contains any crate outside the audited workspace set."""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "Cargo.lock"
ALLOWED = {"bleradar-core", "bleradar-compat"}

names = re.findall(r'^name = "([^"]+)"$', LOCK.read_text(encoding="utf-8"), flags=re.M)
if not names:
    print("Dependency policy gate could not parse any package names from Cargo.lock;")
    print("refusing to pass silently on an unreadable lockfile.")
    sys.exit(1)

foreign = sorted(set(names) - ALLOWED)
if foreign:
    print("Dependency policy violation: third-party crates present in Cargo.lock:")
    for name in foreign:
        print(f"  - {name}")
    print("The workspace is intentionally third-party-free (docs/AUTONOMOUS_DECISIONS.md, decision 9).")
    print("Adding a dependency is a deliberate decision: record it in the decision log and")
    print("update ALLOWED in tools/check_dependency_policy.py in the same change.")
    sys.exit(1)

print(f"Cargo.lock contains only the audited workspace crates: {', '.join(sorted(ALLOWED))}")
