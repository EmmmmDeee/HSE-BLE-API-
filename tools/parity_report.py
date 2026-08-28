#!/usr/bin/env python3
"""Generate an auditable ABI/parity frontier from packaged source and ABI census."""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
ABI = ROOT / "docs" / "NATIVE_ABI.txt"
COMPAT = ROOT / "crates" / "bleradar-compat" / "src" / "lib.rs"
OUT = ROOT / "docs" / "PARITY_COVERAGE.md"

abi_text = ABI.read_text(encoding="utf-8", errors="replace")
compat = COMPAT.read_text(encoding="utf-8")
observed = sorted(set(re.findall(r"UNIFFI_META_BLERADAR_CORE_FUNC_([A-Z0-9_]+)", abi_text)))
observed += sorted(set(re.findall(r"UNIFFI_META_BLERADAR_CORE_(?:METHOD|CONSTRUCTOR)_([A-Z0-9_]+)", abi_text)))
registered = re.findall(r'name: "([^"]+)"', compat)

lines = [
    "# Parity Coverage",
    "",
    "Generated from `docs/NATIVE_ABI.txt` and the semantic compatibility registry.",
    "",
    f"- Observed UniFFI function/method/constructor symbols: **{len(observed)}**",
    f"- Contracts with explicit semantic migration status: **{len(registered)}**",
    f"- Remaining observed symbols requiring semantic registration/characterization: **{max(0, len(observed)-len(registered))}**",
    "",
    "## Registered semantic frontier",
    "",
]
for name in registered:
    lines.append(f"- `{name}`")
lines += [
    "",
    "## Interpretation",
    "",
    "A symbol appearing in the APK is not automatically considered migrated. Exact parity requires characterization of inputs, outputs, side effects, and errors against the immutable oracle. The registry intentionally records that distinction.",
]
OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(OUT)
