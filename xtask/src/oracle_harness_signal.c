/*
 * Executed-oracle signal differential harness (docs/ORACLE_DIFFERENTIAL.md).
 *
 * Compiled for aarch64 with the Android NDK and run under qemu-aarch64 against a
 * real Android Bionic runtime, this program links the IMMUTABLE v0.3.0 native
 * oracle (oracle/libbleradar_core.so) and calls its UniFFI scaffolding for the
 * two core BLE signal contracts, printing one TSV row per call. `cargo xtask
 * oracle-differential` builds and runs it, then compares its output to the
 * committed ground truth
 * (crates/bleradar-compat/tests/oracle/signal_executed_vectors.tsv).
 *
 * ABI (decoded from the oracle's UNIFFI_META_* metadata blobs):
 *   ble_distance(rssi: i32, tx_power: Option<i32>) -> f64
 *   proximity_label(distance_m: f64) -> String
 * A UniFFI Option<i32> argument and a String return are RustBuffers; an Option
 * is lowered as a presence byte then (if present) a big-endian i32, and a String
 * as its raw UTF-8 bytes (length is the RustBuffer's len). Each function takes a
 * trailing RustCallStatus* whose first byte is the status code (0 = success);
 * no JVM is involved.
 *
 * `ble_distance` here always passes tx_power = None (the oracle ignores it,
 * confirmed empirically). Distances are emitted as raw f64 bits so the Rust
 * differential reconstructs the exact input; ble_distance outputs likewise.
 */
#include <stdio.h>
#include <string.h>
#include <stdint.h>

typedef struct {
    uint64_t capacity;
    uint64_t len;
    uint8_t *data;
} RustBuffer;

typedef struct {
    int32_t len;
    const uint8_t *data;
} ForeignBytes;

extern RustBuffer ffi_bleradar_core_rustbuffer_from_bytes(ForeignBytes, void *);
extern double uniffi_bleradar_core_fn_func_ble_distance(int32_t, RustBuffer, void *);
extern RustBuffer uniffi_bleradar_core_fn_func_proximity_label(double, void *);

static uint64_t bits(double value) {
    uint64_t out;
    memcpy(&out, &value, 8);
    return out;
}

/* Builds a UniFFI Option::None RustBuffer for the tx_power argument. */
static RustBuffer none_option(void) {
    unsigned char status[64];
    memset(status, 0, sizeof status);
    unsigned char present = 0;
    ForeignBytes fb;
    fb.len = 1;
    fb.data = &present;
    return ffi_bleradar_core_rustbuffer_from_bytes(fb, status);
}

static void ble(int rssi) {
    unsigned char status[64];
    memset(status, 0, sizeof status);
    double d = uniffi_bleradar_core_fn_func_ble_distance(rssi, none_option(), status);
    printf("BLE\t%d\t%d\t%016llx\n", rssi, (signed char)status[0], (unsigned long long)bits(d));
}

static void prx(double distance) {
    unsigned char status[64];
    memset(status, 0, sizeof status);
    RustBuffer r = uniffi_bleradar_core_fn_func_proximity_label(distance, status);
    char band[64];
    memset(band, 0, sizeof band);
    uint64_t n = r.len < 63 ? r.len : 63;
    if (r.data) {
        memcpy(band, r.data, n);
    }
    printf("PRX\t%016llx\t%d\t%s\n", (unsigned long long)bits(distance), (signed char)status[0], band);
}

int main(void) {
    /* ble_distance over the high-clamp, linear, low-clamp and rssi>=0 sentinel
       regions (calibration is fixed at -59 dBm / 2.4; tx_power ignored). */
    for (int rssi = -130; rssi <= 20; rssi++) {
        ble(rssi);
    }
    /* proximity_label across every band and each transition, at 0.01 m steps
       around the boundaries plus a coarse sweep. */
    for (int i = 0; i <= 2500; i++) {
        prx(i * 0.01);
    }
    return 0;
}
