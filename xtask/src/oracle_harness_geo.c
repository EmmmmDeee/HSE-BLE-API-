/*
 * Executed-oracle geodesy differential harness (docs/ORACLE_DIFFERENTIAL.md).
 *
 * Compiled for aarch64 with the Android NDK and run under qemu-aarch64 against a
 * real Android Bionic runtime, this program links the IMMUTABLE v0.3.0 native
 * oracle (oracle/libbleradar_core.so) and calls its UniFFI scaffolding for the
 * two pure geodesy contracts over a deterministic coordinate sweep, printing one
 * TSV row per pair. `cargo xtask oracle-differential` builds and runs it, then
 * compares its output to the committed ground truth
 * (crates/bleradar-compat/tests/oracle/geodesy_executed_vectors.tsv).
 *
 * ABI (decoded from the oracle's UNIFFI_META_* metadata blobs):
 *   haversine_m(lat1,lon1,lat2,lon2: f64) -> f64   (all doubles; d0..d3 in, d0 out)
 *   bearing_deg(lat1,lon1,lat2,lon2: f64) -> f64
 * Each takes a trailing RustCallStatus* whose first byte is the status code
 * (0 = success); no JVM is involved.
 *
 * Every f64 (both the inputs and the outputs) is emitted as its raw 64-bit hex
 * pattern so the Rust differential reconstructs the exact same f64 with no
 * decimal round-trip loss. Coordinates come from a fixed-seed xorshift* PRNG so
 * the sweep is bit-for-bit reproducible.
 */
#include <stdio.h>
#include <string.h>
#include <stdint.h>

extern double uniffi_bleradar_core_fn_func_haversine_m(double, double, double, double, void *);
extern double uniffi_bleradar_core_fn_func_bearing_deg(double, double, double, double, void *);

static uint64_t rng_state = 0x9e3779b97f4a7c15ULL;

static uint64_t next_u64(void) {
    rng_state ^= rng_state << 13;
    rng_state ^= rng_state >> 7;
    rng_state ^= rng_state << 17;
    return rng_state * 0x2545F4914F6CDD1DULL;
}

/* Uniform in [0, 1) from the top 53 bits. */
static double unit(void) {
    return (double)(next_u64() >> 11) / 9007199254740992.0;
}

static uint64_t bits(double value) {
    uint64_t out;
    memcpy(&out, &value, 8);
    return out;
}

static void row(double lat1, double lon1, double lat2, double lon2) {
    unsigned char status[64];
    memset(status, 0, sizeof status);
    double haversine = uniffi_bleradar_core_fn_func_haversine_m(lat1, lon1, lat2, lon2, status);
    int haversine_code = (signed char)status[0];
    memset(status, 0, sizeof status);
    double bearing = uniffi_bleradar_core_fn_func_bearing_deg(lat1, lon1, lat2, lon2, status);
    int bearing_code = (signed char)status[0];
    printf("GEO\t%016llx\t%016llx\t%016llx\t%016llx\t%d\t%016llx\t%d\t%016llx\n",
           (unsigned long long)bits(lat1), (unsigned long long)bits(lon1),
           (unsigned long long)bits(lat2), (unsigned long long)bits(lon2),
           haversine_code, (unsigned long long)bits(haversine),
           bearing_code, (unsigned long long)bits(bearing));
}

int main(void) {
    /* Explicit edge cases: equator, one-degree steps, identical point, poles,
       the antimeridian, and a near-antimeridian pair. */
    row(0, 0, 1, 0);
    row(0, 0, 0, 1);
    row(0, 0, 0, 0);
    row(90, 0, -90, 0);
    row(0, 180, 0, -180);
    row(89.9, 0, -89.9, 180);
    row(-45, -179.999, 45, 179.999);
    row(51.5, -0.12, 48.85, 2.35);
    /* 300 deterministic pseudo-random valid coordinate pairs. */
    for (int i = 0; i < 300; i++) {
        double lat1 = unit() * 180.0 - 90.0;
        double lon1 = unit() * 360.0 - 180.0;
        double lat2 = unit() * 180.0 - 90.0;
        double lon2 = unit() * 360.0 - 180.0;
        row(lat1, lon1, lat2, lon2);
    }
    return 0;
}
