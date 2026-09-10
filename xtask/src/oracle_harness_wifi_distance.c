/*
 * Executed-oracle wifi_distance differential harness (docs/ORACLE_DIFFERENTIAL.md).
 *
 * Compiled for aarch64 with the Android NDK and run under qemu-aarch64 against a
 * real Android Bionic runtime, this program links the IMMUTABLE v0.3.0 native
 * oracle (oracle/libbleradar_core.so) and calls its UniFFI scaffolding for the
 * previously-unmapped wifi_distance contract over a comprehensive input sweep,
 * printing one TSV row per call. `cargo xtask oracle-differential` builds and
 * runs it, then compares its output to the committed ground truth
 * (crates/bleradar-compat/tests/oracle/wifi_distance_executed_vectors.tsv).
 *
 * ABI (decoded from the oracle's UNIFFI_META_* metadata blobs):
 *   wifi_distance(rssi: i32, frequency_mhz: i32) -> f64
 * Both arguments are plain scalars and the return is a bare C-ABI double (no
 * RustBuffer); the trailing RustCallStatus* first byte is the status code
 * (0 = success). No JVM is involved.
 *
 * Distances are emitted as raw f64 bits (%016llx) so the Rust differential
 * reconstructs the exact value with no decimal-rounding loss. The sweep
 * exercises every region of the contract: the rssi>=0 sentinel, the unclamped
 * log-distance formula, the [0.1, 400] m clamps, the plausible-frequency window
 * edges (2000 and 7199 MHz), and the out-of-window default substitution
 * (including negative and >u16 frequencies the oracle's i32 domain accepts).
 */
#include <stdio.h>
#include <string.h>
#include <stdint.h>

extern double uniffi_bleradar_core_fn_func_wifi_distance(int32_t, int32_t, void *);

static uint64_t bits(double value) {
    uint64_t out;
    memcpy(&out, &value, 8);
    return out;
}

static void wd(int rssi, int freq) {
    unsigned char status[64];
    memset(status, 0, sizeof status);
    double d = uniffi_bleradar_core_fn_func_wifi_distance(rssi, freq, status);
    printf("WD\t%d\t%d\t%d\t%016llx\n", rssi, freq, (signed char)status[0],
           (unsigned long long)bits(d));
}

int main(void) {
    /* 1. rssi sweep at representative in-window frequencies: exercises the
       rssi>=0 sentinel, the unclamped formula, and both distance clamps. */
    int freqs[] = {2412, 2437, 5180, 5825, 5955, 7115, 7199};
    for (unsigned i = 0; i < sizeof freqs / sizeof freqs[0]; i++) {
        for (int rssi = 5; rssi >= -140; rssi--) {
            wd(rssi, freqs[i]);
        }
    }
    /* 2. plausible-frequency window edges (2000 and 7199 MHz), fixed rssi. */
    for (int f = 1996; f <= 2004; f++) {
        wd(-60, f);
    }
    for (int f = 7195; f <= 7204; f++) {
        wd(-60, f);
    }
    /* 3. out-of-window frequencies (small, large, negative, > u16): all default
       to 2437 MHz. Proves the reconstruction's full-i32 domain has no divergence. */
    int outside[] = {0, 1, 100, 1000, 1999, 7200, 10000, 65535, 100000,
                     -1, -2412, -100000, 2000000000, -2000000000};
    for (unsigned i = 0; i < sizeof outside / sizeof outside[0]; i++) {
        wd(-60, outside[i]);
    }
    /* 4. dense in-window frequency sweep across the 2.4/5/6 GHz bands and the
       inter-band gaps, at a fixed mid-range rssi. */
    for (int f = 2400; f <= 2500; f += 5) {
        wd(-70, f);
    }
    for (int f = 5150; f <= 5895; f += 25) {
        wd(-70, f);
    }
    for (int f = 5955; f <= 7115; f += 25) {
        wd(-70, f);
    }
    return 0;
}
