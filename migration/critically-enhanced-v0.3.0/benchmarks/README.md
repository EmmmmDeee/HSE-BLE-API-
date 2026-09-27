# Benchmark harness

No trustworthy legacy source-level benchmark can be reconstructed from the release APK alone.
The original APK and native library are retained under `oracle/` for on-device A/B measurement.
Suggested critical paths: scan ingest throughput, `ui_radar_points`, session serialization,
multilateration, and map overlay generation. No before/after numbers are claimed without an
executable Android test environment.
