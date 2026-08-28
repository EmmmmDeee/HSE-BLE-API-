#!/usr/bin/env python3
"""Fail when a retained binary oracle no longer matches its recorded SHA-256."""
from pathlib import Path
import hashlib
import re
import sys

ROOT = Path(__file__).resolve().parents[1]

# Provenance hash: recorded from the original supplied APK in
# docs/INPUT_SHA256.txt at packaging time.
APK = ROOT / "BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk"
INPUT_SHA = ROOT / "docs" / "INPUT_SHA256.txt"

# Immutability baseline: hash of the migration archive as committed to this
# repository, recorded 2026-08-28. This guards against silent replacement or
# corruption; it is not an original-provenance claim.
ZIP = ROOT / "BLE-Radar-Rust-Migration-Critically-Enhanced-v0.3.0 (1).zip"
ZIP_BASELINE = "07d2d80ce7e6c43f4c6ccc2496d30faafb77342e7bf196b894d32c7528cf3f76"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


failures = []

match = re.search(
    r"Original APK SHA-256:\s*([0-9a-f]{64})",
    INPUT_SHA.read_text(encoding="utf-8") if INPUT_SHA.exists() else "",
)
if not match:
    failures.append(f"could not parse an APK SHA-256 from {INPUT_SHA.name}")
elif not APK.exists():
    failures.append(f"missing oracle file: {APK.name}")
else:
    expected = match.group(1)
    actual = sha256(APK)
    if actual != expected:
        failures.append(f"{APK.name}: expected {expected}, observed {actual}")

if not ZIP.exists():
    failures.append(f"missing oracle archive: {ZIP.name}")
else:
    actual = sha256(ZIP)
    if actual != ZIP_BASELINE:
        failures.append(f"{ZIP.name}: expected {ZIP_BASELINE}, observed {actual}")

if failures:
    print("Oracle integrity violation — immutable behavioral oracles must never change:")
    for failure in failures:
        print(f"  - {failure}")
    sys.exit(1)

print(
    "Oracle integrity verified: APK matches docs/INPUT_SHA256.txt; "
    "migration archive matches its recorded baseline."
)
