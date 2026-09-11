/*
 * Executed-oracle wifi_security / wifi_is_enterprise differential harness
 * (docs/ORACLE_DIFFERENTIAL.md).
 *
 * Compiled for aarch64 with the Android NDK and run under qemu-aarch64 against a
 * real Android Bionic runtime, this program links the IMMUTABLE v0.3.0 native
 * oracle (oracle/libbleradar_core.so) and calls its UniFFI scaffolding for the
 * two previously-unmapped WiFi capability-string contracts over a comprehensive
 * input set, printing one TSV row per input. `cargo xtask oracle-differential`
 * builds and runs it, then compares its output to the committed ground truth
 * (crates/bleradar-compat/tests/oracle/wifi_security_executed_vectors.tsv).
 *
 * ABI (decoded from the oracle's UNIFFI_META_* metadata blobs):
 *   wifi_is_enterprise(caps: Option<String>) -> bool
 *   wifi_security(caps: Option<String>)      -> String
 * A UniFFI Option<String> argument is a RustBuffer: 1 presence byte then, if
 * present, a big-endian i32 length followed by the raw UTF-8 bytes. `bool` is
 * returned as an int8 (0/1); `String` as a RustBuffer of raw UTF-8 (length is
 * the RustBuffer's len). Each function takes a trailing RustCallStatus* whose
 * first byte is the status code (0 = success); no JVM is involved.
 *
 * Row format (caps never contains a tab or newline):
 *   WS  <present 0|1>  <caps>  <is_enterprise 0|1>  <security>
 * present=0 marks the Option::None input (caps column empty).
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
extern int8_t uniffi_bleradar_core_fn_func_wifi_is_enterprise(RustBuffer, void *);
extern RustBuffer uniffi_bleradar_core_fn_func_wifi_security(RustBuffer, void *);

/* Builds a UniFFI Option<String> RustBuffer (None or Some(utf8)). */
static RustBuffer option_string(int present, const char *s) {
    unsigned char status[64];
    memset(status, 0, sizeof status);
    unsigned char buf[1024];
    int n = 0;
    if (!present) {
        buf[0] = 0;
        n = 1;
    } else {
        int len = (int)strlen(s);
        buf[0] = 1;
        buf[1] = (len >> 24) & 0xff;
        buf[2] = (len >> 16) & 0xff;
        buf[3] = (len >> 8) & 0xff;
        buf[4] = len & 0xff;
        memcpy(buf + 5, s, (size_t)len);
        n = 5 + len;
    }
    ForeignBytes fb;
    fb.len = n;
    fb.data = buf;
    return ffi_bleradar_core_rustbuffer_from_bytes(fb, status);
}

static void row(int present, const char *caps) {
    unsigned char status[64];
    memset(status, 0, sizeof status);
    int8_t ent = uniffi_bleradar_core_fn_func_wifi_is_enterprise(option_string(present, caps), status);
    int ent_status = (signed char)status[0];
    memset(status, 0, sizeof status);
    RustBuffer r = uniffi_bleradar_core_fn_func_wifi_security(option_string(present, caps), status);
    int sec_status = (signed char)status[0];
    char sec[128];
    memset(sec, 0, sizeof sec);
    uint64_t k = r.len < 127 ? r.len : 127;
    if (r.data) {
        memcpy(sec, r.data, k);
    }
    /* Both statuses must be 0; emit the enterprise status so drift/faults show. */
    printf("WS\t%d\t%s\t%d\t%s\n", present, present ? caps : "",
           (ent_status == 0 && sec_status == 0) ? (ent ? 1 : 0) : -1, sec);
}

int main(void) {
    const char *caps[] = {
        /* open / no security token */
        "[ESS]", "[IBSS]", "", "[ESS][WPS]", "random", "eap", "wpa", "rsn", "sae",
        /* WEP */
        "[WEP]", "[WEP][ESS]",
        /* WPA (not WPA2/3) */
        "[WPA-PSK-TKIP][ESS]", "[WPA-PSK-CCMP][ESS]", "[WPA-EAP-TKIP][ESS]", "WPA",
        /* WPA2 / RSN */
        "[WPA2-PSK-CCMP][ESS]", "[WPA2-PSK-CCMP+TKIP][ESS]", "[WPA2-EAP-CCMP][ESS]",
        "[WPA2-EAP-CCMP+FT/EAP][ESS]", "[WPA2-EAP-SUITE-B-192][ESS]",
        "[WPA2-EAP/SHA256-CCMP][ESS]", "[WPA2-FT/EAP-CCMP][ESS]",
        "[RSN-PSK-CCMP][ESS]", "[RSN-OWE-CCMP][ESS]", "RSN", "WPA2-PSK",
        "[WPA-PSK-TKIP][WPA2-PSK-CCMP][ESS]",
        /* WPA3 / SAE (incl. mixed and literal WPA3) */
        "[RSN-SAE-CCMP][ESS]", "[WPA3-SAE][ESS]", "[RSN-SAE+FT/SAE-CCMP][ESS]",
        "[WPA3-XYZ][ESS]", "[WPA2-SAE-CCMP][ESS]", "[SAE][ESS]", "SAE",
        "[WPA2-PSK-CCMP][RSN-SAE-CCMP][ESS]", "[WPA2-EAP-CCMP][RSN-SAE-CCMP][ESS]",
        /* OWE (enhanced open) */
        "[OWE][ESS]", "[OWE-CCMP][ESS]", "OWE",
        /* precedence mixes */
        "[WPA2-PSK-CCMP][WEP][ESS]", "[WEP][WPA-PSK][ESS]",
        /* enterprise substring edge cases (case-sensitive EAP) */
        "[FT/EAP][ESS]", "MYEAPX", "eapol", "wpa2-eap",
    };
    for (unsigned i = 0; i < sizeof caps / sizeof caps[0]; i++) {
        row(1, caps[i]);
    }
    /* Option::None */
    row(0, "");
    return 0;
}
