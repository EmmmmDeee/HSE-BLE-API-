# Autonomous Decision Log

- Preserved the exact supplied APK and native Rust `.so` as immutable behavior oracles before reconstruction.
- Chose a zero-third-party-dependency Rust workspace to maximize deterministic, offline buildability and eliminate new dependency CVEs.
- Refused to guess undocumented legacy semantics; unknown contracts are inventoried as compatibility gaps.
- Reconstructed only mathematically/structurally high-confidence behavior: geographic validation, haversine distance, initial bearing, MAC canonicalization/privacy bit, basic Wi-Fi channel conversion, deterministic RSSI filtering/trend classification.
- Did not modify or re-sign the original APK because the signing key is unavailable.
- Did not import credentials from unrelated sources; binary credential census found no populated user secrets.
- Pinned Rust 1.98.0, the current stable release as of 2026-08-27.
- Created a git recovery history locally before final packaging.
