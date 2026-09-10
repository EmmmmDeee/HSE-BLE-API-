/*
 * Executed-oracle differential harness (docs/ORACLE_DIFFERENTIAL.md).
 *
 * Compiled for aarch64 with the Android NDK and run under qemu-aarch64 against
 * a real Android Bionic runtime, this program links the IMMUTABLE v0.3.0 native
 * oracle (oracle/libbleradar_core.so) and calls its UniFFI scaffolding for the
 * WiFi channel<->frequency contracts over a comprehensive input sweep, printing
 * one TSV row per call. `cargo xtask oracle-differential` builds and runs it,
 * then compares its output to the committed ground truth
 * (crates/bleradar-compat/tests/oracle/wifi_executed_vectors.tsv).
 *
 * ABI notes (decoded from the oracle's UNIFFI_META_* metadata blobs):
 *   wifi_channel_to_frequency(rssi: i32)            -> Option<i32>  (RustBuffer)
 *   wifi_frequency_to_channel(mhz: Option<i32>)     -> Option<i32>  (RustBuffer)
 * A UniFFI Option<i32> is serialized as 1 presence byte then, if present, a
 * big-endian i32. Scaffolding functions take a trailing RustCallStatus* whose
 * first byte is the status code (0 = success); no JVM is involved.
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
extern RustBuffer uniffi_bleradar_core_fn_func_wifi_channel_to_frequency(int32_t, void *);
extern RustBuffer uniffi_bleradar_core_fn_func_wifi_frequency_to_channel(RustBuffer, void *);
extern RustBuffer uniffi_bleradar_core_fn_func_wifi_band(RustBuffer, void *);

/* Prints a decoded UniFFI Option<i32> as `None` or its decimal value. */
static void emit_option(const RustBuffer *r, int status) {
    if (status != 0 || r->data == 0 || r->len < 1 || r->data[0] == 0) {
        printf("None\n");
        return;
    }
    if (r->len < 5) {
        printf("None\n");
        return;
    }
    long v = ((long)r->data[1] << 24) | ((long)r->data[2] << 16)
           | ((long)r->data[3] << 8) | (long)r->data[4];
    printf("%ld\n", v);
}

static void wifi_channel_to_frequency(int channel) {
    unsigned char status[64];
    memset(status, 0, sizeof status);
    RustBuffer r = uniffi_bleradar_core_fn_func_wifi_channel_to_frequency(channel, status);
    printf("WCF\t%d\t", channel);
    emit_option(&r, (signed char)status[0]);
}

/* Builds a UniFFI Option<i32> RustBuffer (None or Some(value)). */
static RustBuffer option_i32(int has_value, long value) {
    unsigned char status[64];
    memset(status, 0, sizeof status);
    unsigned char in[5];
    int in_len;
    if (!has_value) {
        in[0] = 0;
        in_len = 1;
    } else {
        in[0] = 1;
        in[1] = (value >> 24) & 0xff;
        in[2] = (value >> 16) & 0xff;
        in[3] = (value >> 8) & 0xff;
        in[4] = value & 0xff;
        in_len = 5;
    }
    ForeignBytes fb;
    fb.len = in_len;
    fb.data = in;
    return ffi_bleradar_core_rustbuffer_from_bytes(fb, status);
}

static void wifi_frequency_to_channel(int has_value, long mhz) {
    unsigned char status[64];
    RustBuffer arg = option_i32(has_value, mhz);
    memset(status, 0, sizeof status);
    RustBuffer r = uniffi_bleradar_core_fn_func_wifi_frequency_to_channel(arg, status);
    if (has_value) {
        printf("WFC\t%ld\t", mhz);
    } else {
        printf("WFC\tNone\t");
    }
    emit_option(&r, (signed char)status[0]);
}

static void wifi_band(int has_value, long mhz) {
    unsigned char status[64];
    RustBuffer arg = option_i32(has_value, mhz);
    memset(status, 0, sizeof status);
    RustBuffer r = uniffi_bleradar_core_fn_func_wifi_band(arg, status);
    char band[64];
    memset(band, 0, sizeof band);
    unsigned long n = r.len < 63 ? (unsigned long)r.len : 63;
    if ((signed char)status[0] == 0 && r.data) {
        memcpy(band, r.data, n);
    }
    if (has_value) {
        printf("WBAND\t%ld\t%s\n", mhz, band);
    } else {
        printf("WBAND\tNone\t%s\n", band);
    }
}

int main(void) {
    /* channel -> frequency: exhaustive over the practical and boundary domain */
    for (int ch = -8; ch <= 200; ch++) {
        wifi_channel_to_frequency(ch);
    }
    /* frequency -> channel: None sentinel, out-of-u16 extremes, then 1 MHz
       sweeps across every band edge and its floor-division behavior. */
    wifi_frequency_to_channel(0, 0);
    wifi_frequency_to_channel(1, -2000000000);
    wifi_frequency_to_channel(1, -1);
    wifi_frequency_to_channel(1, 0);
    wifi_frequency_to_channel(1, 100000);
    wifi_frequency_to_channel(1, 2000000000);
    for (long m = 2405; m <= 2495; m++) {
        wifi_frequency_to_channel(1, m); /* 2.4 GHz band + edges + flooring */
    }
    for (long m = 5150; m <= 5260; m++) {
        wifi_frequency_to_channel(1, m); /* 5 GHz low edge + flooring */
    }
    for (long m = 5875; m <= 5895; m++) {
        wifi_frequency_to_channel(1, m); /* 5 GHz high edge */
    }
    for (long m = 5945; m <= 6065; m++) {
        wifi_frequency_to_channel(1, m); /* 6 GHz low edge + flooring */
    }
    for (long m = 7105; m <= 7125; m++) {
        wifi_frequency_to_channel(1, m); /* 6 GHz high edge */
    }
    /* band label: None sentinel, out-of-u16 and negative inputs, dense sweeps
       across both band boundaries (3000, 5900 MHz), and high-u16 samples. */
    wifi_band(0, 0);
    wifi_band(1, -1000);
    wifi_band(1, -1);
    wifi_band(1, 0);
    wifi_band(1, 1);
    for (long m = 2900; m <= 3100; m++) {
        wifi_band(1, m); /* 2.4/5 GHz boundary at 3000 */
    }
    for (long m = 5800; m <= 6000; m++) {
        wifi_band(1, m); /* 5/6 GHz boundary at 5900 */
    }
    long band_samples[] = {2412, 2484, 5180, 5825, 5955, 7115, 8000, 10000, 20000, 40000, 65535, 100000};
    for (unsigned i = 0; i < sizeof band_samples / sizeof band_samples[0]; i++) {
        wifi_band(1, band_samples[i]);
    }
    return 0;
}
