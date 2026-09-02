//! Executable models and fixtures for stateless v0.3.0 native-oracle exports.

use bleradar_compat::{Reachability, ReachabilityEvidence, runtime_contract};

const TRACE_VERIFIED_STATELESS_CONTRACTS: [&str; 22] = [
    "bt_category_from_class",
    "bt_major",
    "bt_major_label",
    "bt_minor",
    "device_category",
    "export_csv_field",
    "fmt_rssi",
    "gatt_short",
    "import_parse_json",
    "import_parse_wigle",
    "import_records",
    "import_split_csv",
    "mac_info",
    "oui_vendor",
    "session_filter_sort",
    "session_fingerprint",
    "session_summaries",
    "times_iso",
    "times_parse_iso",
    "times_parse_wigle",
    "ui_point_alpha",
    "ui_stable_angle",
];

fn oracle_bt_major(class: i32) -> i32 {
    ((class as u32 >> 8) & 0x1f) as i32
}

fn oracle_bt_minor(class: i32) -> i32 {
    ((class as u32 >> 2) & 0x3f) as i32
}

fn oracle_bt_major_label(class: Option<i32>) -> &'static str {
    let Some(class) = class else {
        return "Unknown";
    };
    match oracle_bt_major(class) {
        0 => "Miscellaneous",
        1 => "Computer",
        2 => "Phone",
        3 => "LAN / network AP",
        4 => "Audio / video",
        5 => "Peripheral",
        6 => "Imaging",
        7 => "Wearable",
        8 => "Toy",
        9 => "Health",
        _ => "Uncategorised",
    }
}

fn oracle_bt_category(class: Option<i32>) -> Option<&'static str> {
    match oracle_bt_major(class?) {
        1 => Some("computer"),
        2 => Some("phone"),
        3 => Some("router_ap"),
        4 => Some("audio"),
        5 => Some("input"),
        6 => Some("printer"),
        7 => Some("wearable"),
        8 => Some("gaming"),
        9 => Some("medical"),
        _ => None,
    }
}

fn oracle_csv_field(input: Option<&str>) -> String {
    let input = input.unwrap_or_default();
    if input
        .chars()
        .any(|ch| matches!(ch, ',' | '"' | '\r' | '\n'))
    {
        format!("\"{}\"", input.replace('"', "\"\""))
    } else {
        input.to_owned()
    }
}

fn oracle_fmt_rssi(value: i32) -> String {
    format!("{value} dBm")
}

fn oracle_gatt_short(input: &str) -> String {
    let lower = input.to_lowercase();
    if lower.len() == 36
        && lower.starts_with("0000")
        && lower.ends_with("-0000-1000-8000-00805f9b34fb")
        && let Some(short) = lower.get(4..8)
    {
        format!("0x{short}")
    } else {
        lower
    }
}

fn oracle_session_fingerprint(transport: &str, address: &str) -> String {
    format!("{transport}:{}", address.to_uppercase())
}

fn oracle_point_alpha(now_ms: i64, last_seen_ms: i64) -> f32 {
    let age_ms = now_ms.wrapping_sub(last_seen_ms).clamp(0, 60_000);
    (age_ms as f32 / -60_000.0) * 0.7 + 1.0
}

fn oracle_stable_angle(address: &str) -> f32 {
    let hash = address
        .encode_utf16()
        .fold(1_125_899_906_842_597_u64, |hash, code_unit| {
            hash.wrapping_mul(31).wrapping_add(u64::from(code_unit))
        });
    ((hash >> 1) % 3_600) as f32 / 10.0
}

#[test]
fn every_new_stateless_trace_has_runtime_evidence() {
    for name in TRACE_VERIFIED_STATELESS_CONTRACTS {
        let contract = runtime_contract(name).unwrap();
        assert_eq!(
            contract.reachability,
            Reachability::VerifiedRuntime,
            "{name}"
        );
        assert_eq!(
            contract.evidence,
            ReachabilityEvidence::InstrumentedRuntimeTrace,
            "{name}"
        );
    }
}

#[test]
fn bluetooth_class_bitfields_labels_and_categories_are_locked() {
    assert_eq!(oracle_bt_major(i32::MIN), 0);
    assert_eq!(oracle_bt_minor(i32::MIN), 0);
    assert_eq!(oracle_bt_major(-1), 31);
    assert_eq!(oracle_bt_minor(-1), 63);

    let labels = [
        "Miscellaneous",
        "Computer",
        "Phone",
        "LAN / network AP",
        "Audio / video",
        "Peripheral",
        "Imaging",
        "Wearable",
        "Toy",
        "Health",
    ];
    for (major, label) in labels.into_iter().enumerate() {
        let class = i32::try_from(major).unwrap() << 8;
        assert_eq!(oracle_bt_major_label(Some(class)), label);
    }
    assert_eq!(oracle_bt_major_label(Some(10 << 8)), "Uncategorised");
    assert_eq!(oracle_bt_major_label(None), "Unknown");

    let categories = [
        None,
        Some("computer"),
        Some("phone"),
        Some("router_ap"),
        Some("audio"),
        Some("input"),
        Some("printer"),
        Some("wearable"),
        Some("gaming"),
        Some("medical"),
    ];
    for (major, category) in categories.into_iter().enumerate() {
        let class = i32::try_from(major).unwrap() << 8;
        assert_eq!(oracle_bt_category(Some(class)), category);
    }
    assert_eq!(oracle_bt_category(None), None);
}

#[test]
fn csv_quoting_and_text_normalization_are_locked() {
    for (input, expected) in [
        (None, ""),
        (Some(""), ""),
        (Some("abc"), "abc"),
        (Some("a,b"), "\"a,b\""),
        (Some("a\"b"), "\"a\"\"b\""),
        (Some("a\nb"), "\"a\nb\""),
        (Some("a\rb"), "\"a\rb\""),
        (Some("a\tb"), "a\tb"),
        (Some("=1+1"), "=1+1"),
    ] {
        assert_eq!(oracle_csv_field(input), expected);
    }

    for (value, expected) in [
        (i32::MIN, "-2147483648 dBm"),
        (-101, "-101 dBm"),
        (0, "0 dBm"),
        (127, "127 dBm"),
        (i32::MAX, "2147483647 dBm"),
    ] {
        assert_eq!(oracle_fmt_rssi(value), expected);
    }

    assert_eq!(
        oracle_gatt_short("0000180A-0000-1000-8000-00805F9B34FB"),
        "0x180a"
    );
    assert_eq!(
        oracle_gatt_short("12345678-1234-5678-9ABC-DEF012345678"),
        "12345678-1234-5678-9abc-def012345678"
    );
    assert_eq!(
        oracle_gatt_short("00001800x-0000-1000-8000-00805f9b34fb"),
        "00001800x-0000-1000-8000-00805f9b34fb"
    );
    assert_eq!(
        oracle_gatt_short("0000zzzz-0000-1000-8000-00805f9b34fb"),
        "0xzzzz"
    );
    assert_eq!(
        oracle_session_fingerprint("ble", "aa:bb:cc:dd:ee:ff"),
        "ble:AA:BB:CC:DD:EE:FF"
    );
    assert_eq!(oracle_session_fingerprint("x", "straße"), "x:STRASSE");
}

#[test]
fn projection_math_is_locked_at_boundaries_and_unicode_inputs() {
    for (now_ms, last_seen_ms, expected) in [
        (0, 0, 1.0_f32),
        (1_000, 0, 0.988_333_34_f32),
        (30_000, 0, 0.65_f32),
        (60_000, 0, 0.3_f32),
        (60_001, 0, 0.3_f32),
        (1_000, 2_000, 1.0_f32),
        (i64::MIN, i64::MAX, 0.999_988_3_f32),
        (i64::MAX, i64::MIN, 1.0_f32),
    ] {
        assert_eq!(
            oracle_point_alpha(now_ms, last_seen_ms).to_bits(),
            expected.to_bits()
        );
    }

    for (address, expected) in [
        ("", 169.8_f32),
        ("a", 230.2_f32),
        ("A", 228.6_f32),
        ("aa", 301.0_f32),
        ("ble:aa:bb:cc:dd:ee:ff", 181.9_f32),
        ("wifi:Café", 328.0_f32),
        ("😀", 230.8_f32),
    ] {
        assert_eq!(oracle_stable_angle(address).to_bits(), expected.to_bits());
    }
}
