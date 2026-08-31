//! Minimal DEX (`classes.dex`) class-definition census reader.
//!
//! Parses just enough of the Dalvik executable format — header, string IDs,
//! type IDs, and class defs — to answer one question completely and
//! correctly: which classes does this DEX define? This underpins
//! `docs/DEX_CLASS_CENSUS.txt`; see `docs/AUTONOMOUS_DECISIONS.md` decision
//! 26 for why the prior two-line census was regenerated from first
//! principles instead of trusted. Method bodies, bytecode, and non-class
//! metadata are out of scope for this census and are not parsed.
//!
//! String decoding uses DEX's "modified UTF-8" (MUTF-8): identical to
//! standard UTF-8 for every codepoint that can appear in a class descriptor
//! in practice (ASCII package/class names), so `str::from_utf8` succeeds for
//! all observed input; the rare supplementary-plane surrogate-pair encoding
//! that differs from standard UTF-8 falls back to a lossy conversion rather
//! than failing the census (this module reports class descriptors for
//! evidence, not a byte-exact DEX round-trip reconstruction).

use std::fmt;

/// A failure while parsing a DEX file's class census.
#[derive(Debug)]
pub enum DexError {
    /// The input is shorter than a fixed-size record it claims to contain,
    /// or a table offset/index falls outside the file.
    Truncated(&'static str),
    /// The file does not start with a recognized `dex\n0` magic.
    NotDex,
}

impl fmt::Display for DexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DexError::Truncated(what) => write!(f, "truncated DEX file: {what}"),
            DexError::NotDex => write!(f, "not a DEX file (bad magic)"),
        }
    }
}

impl std::error::Error for DexError {}

const HEADER_LEN: usize = 112;

fn u32_at(data: &[u8], off: usize, what: &'static str) -> Result<u32, DexError> {
    let bytes: [u8; 4] = data
        .get(off..off + 4)
        .ok_or(DexError::Truncated(what))?
        .try_into()
        .map_err(|_| DexError::Truncated(what))?;
    Ok(u32::from_le_bytes(bytes))
}

/// Reads a ULEB128-encoded value starting at `off`, returning the value and
/// the offset of the first byte after it.
fn uleb128(data: &[u8], mut off: usize) -> Result<(u64, usize), DexError> {
    let mut result: u64 = 0;
    let mut shift = 0;
    loop {
        let byte = *data.get(off).ok_or(DexError::Truncated("uleb128"))?;
        off += 1;
        result |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    Ok((result, off))
}

/// Decodes the MUTF-8 string stored at `string_data_off` (a ULEB128
/// `utf16_size` followed by NUL-terminated modified-UTF-8 bytes).
fn mutf8_string_at(data: &[u8], string_data_off: usize) -> Result<String, DexError> {
    let (_utf16_size, bytes_start) = uleb128(data, string_data_off)?;
    let rest = data
        .get(bytes_start..)
        .ok_or(DexError::Truncated("string data"))?;
    let len = rest
        .iter()
        .position(|&b| b == 0)
        .ok_or(DexError::Truncated("unterminated string data"))?;
    let raw = &rest[..len];
    Ok(match std::str::from_utf8(raw) {
        Ok(s) => s.to_string(),
        Err(_) => String::from_utf8_lossy(raw).into_owned(),
    })
}

/// Strips a JVM/Dalvik class type descriptor's `L...;` wrapper down to its
/// internal (slash-separated) binary name, matching the convention already
/// established in `docs/DEX_CLASS_CENSUS.txt`. Descriptors that do not match
/// the expected `L...;` shape (which should not occur for `class_def`
/// entries) are returned unchanged rather than mangled.
fn strip_class_descriptor(descriptor: &str) -> &str {
    descriptor
        .strip_prefix('L')
        .and_then(|s| s.strip_suffix(';'))
        .unwrap_or(descriptor)
}

/// Returns the sorted, deduplicated list of internal class names
/// (`com/example/Foo` form) that `data`'s DEX `class_defs` table defines.
pub fn class_names(data: &[u8]) -> Result<Vec<String>, DexError> {
    if data.len() < HEADER_LEN || &data[0..4] != b"dex\n" {
        return Err(DexError::NotDex);
    }

    let string_ids_size = u32_at(data, 56, "string_ids_size")? as usize;
    let string_ids_off = u32_at(data, 60, "string_ids_off")? as usize;
    let type_ids_size = u32_at(data, 64, "type_ids_size")? as usize;
    let type_ids_off = u32_at(data, 68, "type_ids_off")? as usize;
    let class_defs_size = u32_at(data, 96, "class_defs_size")? as usize;
    let class_defs_off = u32_at(data, 100, "class_defs_off")? as usize;

    // string_ids: array of uint32 string_data_off, 4 bytes each.
    let read_string = |string_idx: usize| -> Result<String, DexError> {
        if string_idx >= string_ids_size {
            return Err(DexError::Truncated("string_ids index out of range"));
        }
        let entry_off = string_ids_off + string_idx * 4;
        let string_data_off = u32_at(data, entry_off, "string_ids entry")? as usize;
        mutf8_string_at(data, string_data_off)
    };

    // type_ids: array of uint32 descriptor_idx (into string_ids), 4 bytes each.
    let read_type_descriptor = |type_idx: usize| -> Result<String, DexError> {
        if type_idx >= type_ids_size {
            return Err(DexError::Truncated("type_ids index out of range"));
        }
        let entry_off = type_ids_off + type_idx * 4;
        let descriptor_idx = u32_at(data, entry_off, "type_ids entry")? as usize;
        read_string(descriptor_idx)
    };

    // class_def_item is 32 bytes; class_idx (into type_ids) is the first uint32.
    const CLASS_DEF_LEN: usize = 32;
    let mut names = Vec::with_capacity(class_defs_size);
    for i in 0..class_defs_size {
        let base = class_defs_off + i * CLASS_DEF_LEN;
        let class_idx = u32_at(data, base, "class_def class_idx")? as usize;
        let descriptor = read_type_descriptor(class_idx)?;
        names.push(strip_class_descriptor(&descriptor).to_string());
    }

    names.sort();
    names.dedup();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal DEX with two classes (`Lcom/example/Foo;` and
    /// `Lcom/example/Bar;`) to exercise the string/type/class_def walk
    /// without depending on the retained oracle DEX.
    fn build_test_dex() -> Vec<u8> {
        // Build the string data blob first while recording each string's
        // offset within the blob, since the class_defs table needs to
        // reference absolute file offsets computed from the final layout.
        let mut string_offsets = Vec::new();
        let mut blob = Vec::new();
        for s in [
            "Lcom/example/Foo;",
            "Lcom/example/Bar;",
            "Ljava/lang/Object;",
        ] {
            string_offsets.push(blob.len());
            blob.push(s.encode_utf16().count() as u8); // utf16_size (single-byte uleb128; test strings are short)
            blob.extend_from_slice(s.as_bytes());
            blob.push(0);
        }

        // Layout: header | string_ids (3*4) | type_ids (3*4) | class_defs (2*32) | string data blob
        let string_ids_off = HEADER_LEN;
        let type_ids_off = string_ids_off + 3 * 4;
        let class_defs_off = type_ids_off + 3 * 4;
        let strings_blob_off = class_defs_off + 2 * 32;

        let mut data = vec![0u8; strings_blob_off];
        data[0..4].copy_from_slice(b"dex\n");
        data[4..8].copy_from_slice(b"037\0");

        // string_ids: absolute offsets into the trailing blob.
        for (i, &rel) in string_offsets.iter().enumerate() {
            let off = string_ids_off + i * 4;
            let abs = (strings_blob_off + rel) as u32;
            data[off..off + 4].copy_from_slice(&abs.to_le_bytes());
        }

        // type_ids: descriptor_idx 0 -> Foo, 1 -> Bar, 2 -> Object.
        for (i, string_idx) in [0u32, 1, 2].into_iter().enumerate() {
            let off = type_ids_off + i * 4;
            data[off..off + 4].copy_from_slice(&string_idx.to_le_bytes());
        }

        // class_defs: two classes, both with superclass type_idx 2 (Object).
        for (i, class_type_idx) in [0u32, 1].into_iter().enumerate() {
            let base = class_defs_off + i * 32;
            data[base..base + 4].copy_from_slice(&class_type_idx.to_le_bytes()); // class_idx
            data[base + 8..base + 12].copy_from_slice(&2u32.to_le_bytes()); // superclass_idx
            // Remaining fields (access_flags, interfaces_off, source_file_idx,
            // annotations_off, class_data_off, static_values_off) stay zero.
        }

        data.extend_from_slice(&blob);

        data[56..60].copy_from_slice(&3u32.to_le_bytes()); // string_ids_size
        data[60..64].copy_from_slice(&(string_ids_off as u32).to_le_bytes());
        data[64..68].copy_from_slice(&3u32.to_le_bytes()); // type_ids_size
        data[68..72].copy_from_slice(&(type_ids_off as u32).to_le_bytes());
        data[96..100].copy_from_slice(&2u32.to_le_bytes()); // class_defs_size
        data[100..104].copy_from_slice(&(class_defs_off as u32).to_le_bytes());

        data
    }

    #[test]
    fn extracts_sorted_class_names() {
        let dex = build_test_dex();
        let names = class_names(&dex).expect("parse succeeds");
        assert_eq!(
            names,
            vec!["com/example/Bar".to_string(), "com/example/Foo".to_string()]
        );
    }

    #[test]
    fn rejects_non_dex_input() {
        assert!(matches!(
            class_names(b"not a dex file"),
            Err(DexError::NotDex)
        ));
    }

    #[test]
    fn strips_l_semicolon_wrapper() {
        assert_eq!(
            strip_class_descriptor("Lcom/example/Foo;"),
            "com/example/Foo"
        );
        assert_eq!(strip_class_descriptor("malformed"), "malformed");
    }
}
