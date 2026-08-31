//! Minimal ZIP central-directory reader.
//!
//! Reproduces exactly what `tools/apk_inventory.py` needs from
//! `zipfile.ZipFile(apk).namelist()`: the list of entry names recorded in the
//! central directory. Entry *names* in a ZIP's central directory are always
//! stored uncompressed, so — unlike entry contents, where `classes.dex` uses
//! Deflate — no decompressor is required to reproduce this census (see
//! `docs/AUTONOMOUS_DECISIONS.md` decision 25). ZIP64 is intentionally out of
//! scope: the retained oracle APK is well under the 4 GiB / 65535-entry
//! ZIP64 thresholds, and the parser reports [`ZipError::Zip64Unsupported`]
//! rather than silently truncating if it ever encountered one.

use std::fmt;

/// A failure while parsing a ZIP archive's central directory.
#[derive(Debug)]
pub enum ZipError {
    /// No end-of-central-directory record was found.
    NoEndOfCentralDirectory,
    /// The archive claims ZIP64 sentinel values, which this reader does not
    /// support (see module docs).
    Zip64Unsupported,
    /// A central directory record was shorter than its fixed-size header.
    Truncated(&'static str),
    /// A central directory header had a bad signature.
    BadSignature,
}

impl fmt::Display for ZipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZipError::NoEndOfCentralDirectory => {
                write!(f, "no end-of-central-directory record found")
            }
            ZipError::Zip64Unsupported => write!(f, "ZIP64 archives are not supported"),
            ZipError::Truncated(what) => write!(f, "truncated ZIP central directory: {what}"),
            ZipError::BadSignature => write!(f, "bad central directory file header signature"),
        }
    }
}

impl std::error::Error for ZipError {}

const EOCD_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const CENTRAL_DIR_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
const EOCD_FIXED_LEN: usize = 22;
const CENTRAL_DIR_FIXED_LEN: usize = 46;

/// Locates the end-of-central-directory record by scanning backward from the
/// end of the file (it may be followed by a variable-length archive comment,
/// so its position cannot be computed directly).
fn find_eocd(data: &[u8]) -> Result<usize, ZipError> {
    if data.len() < EOCD_FIXED_LEN {
        return Err(ZipError::NoEndOfCentralDirectory);
    }
    // Max comment length is u16::MAX; search no further back than that.
    let search_floor = data.len().saturating_sub(EOCD_FIXED_LEN + 0xffff);
    let mut i = data.len() - EOCD_FIXED_LEN;
    loop {
        if data[i..i + 4] == EOCD_SIGNATURE {
            return Ok(i);
        }
        if i == search_floor {
            break;
        }
        i -= 1;
    }
    Err(ZipError::NoEndOfCentralDirectory)
}

/// Returns every entry name recorded in `data`'s ZIP central directory, in
/// on-disk (central directory) order.
pub fn entry_names(data: &[u8]) -> Result<Vec<String>, ZipError> {
    let eocd = find_eocd(data)?;
    let total_entries = u16::from_le_bytes([data[eocd + 10], data[eocd + 11]]) as usize;
    let cd_size = u32::from_le_bytes([
        data[eocd + 12],
        data[eocd + 13],
        data[eocd + 14],
        data[eocd + 15],
    ]);
    let cd_offset = u32::from_le_bytes([
        data[eocd + 16],
        data[eocd + 17],
        data[eocd + 18],
        data[eocd + 19],
    ]);
    if total_entries == 0xffff || cd_size == 0xffff_ffff || cd_offset == 0xffff_ffff {
        return Err(ZipError::Zip64Unsupported);
    }

    let mut names = Vec::with_capacity(total_entries);
    let mut pos = cd_offset as usize;
    for _ in 0..total_entries {
        let header = data
            .get(pos..pos + CENTRAL_DIR_FIXED_LEN)
            .ok_or(ZipError::Truncated("central directory file header"))?;
        if header[0..4] != CENTRAL_DIR_SIGNATURE {
            return Err(ZipError::BadSignature);
        }
        let name_len = u16::from_le_bytes([header[28], header[29]]) as usize;
        let extra_len = u16::from_le_bytes([header[30], header[31]]) as usize;
        let comment_len = u16::from_le_bytes([header[32], header[33]]) as usize;

        let name_start = pos + CENTRAL_DIR_FIXED_LEN;
        let name_bytes = data
            .get(name_start..name_start + name_len)
            .ok_or(ZipError::Truncated("file name"))?;
        // APK/ZIP entry names are conventionally UTF-8 (or plain ASCII);
        // fall back to lossy conversion rather than failing the whole
        // census over a single exotic name.
        names.push(String::from_utf8_lossy(name_bytes).into_owned());

        pos = name_start + name_len + extra_len + comment_len;
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal valid ZIP (two stored, empty-content entries) to
    /// exercise the central-directory walk end to end.
    fn build_test_zip(names: &[&str]) -> Vec<u8> {
        let mut data = Vec::new();
        let mut local_offsets = Vec::new();

        for name in names {
            local_offsets.push(data.len() as u32);
            data.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]); // local file header sig
            data.extend_from_slice(&0u16.to_le_bytes()); // version needed
            data.extend_from_slice(&0u16.to_le_bytes()); // flags
            data.extend_from_slice(&0u16.to_le_bytes()); // method: stored
            data.extend_from_slice(&0u16.to_le_bytes()); // mod time
            data.extend_from_slice(&0u16.to_le_bytes()); // mod date
            data.extend_from_slice(&0u32.to_le_bytes()); // crc32
            data.extend_from_slice(&0u32.to_le_bytes()); // compressed size
            data.extend_from_slice(&0u32.to_le_bytes()); // uncompressed size
            data.extend_from_slice(&(name.len() as u16).to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes()); // extra len
            data.extend_from_slice(name.as_bytes());
        }

        let cd_start = data.len() as u32;
        for (name, &offset) in names.iter().zip(local_offsets.iter()) {
            data.extend_from_slice(&CENTRAL_DIR_SIGNATURE);
            data.extend_from_slice(&0u16.to_le_bytes()); // version made by
            data.extend_from_slice(&0u16.to_le_bytes()); // version needed
            data.extend_from_slice(&0u16.to_le_bytes()); // flags
            data.extend_from_slice(&0u16.to_le_bytes()); // method
            data.extend_from_slice(&0u16.to_le_bytes()); // mod time
            data.extend_from_slice(&0u16.to_le_bytes()); // mod date
            data.extend_from_slice(&0u32.to_le_bytes()); // crc32
            data.extend_from_slice(&0u32.to_le_bytes()); // compressed size
            data.extend_from_slice(&0u32.to_le_bytes()); // uncompressed size
            data.extend_from_slice(&(name.len() as u16).to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes()); // extra len
            data.extend_from_slice(&0u16.to_le_bytes()); // comment len
            data.extend_from_slice(&0u16.to_le_bytes()); // disk start
            data.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            data.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            data.extend_from_slice(&offset.to_le_bytes());
            data.extend_from_slice(name.as_bytes());
        }
        let cd_size = data.len() as u32 - cd_start;

        data.extend_from_slice(&EOCD_SIGNATURE);
        data.extend_from_slice(&0u16.to_le_bytes()); // disk number
        data.extend_from_slice(&0u16.to_le_bytes()); // disk with cd
        data.extend_from_slice(&(names.len() as u16).to_le_bytes());
        data.extend_from_slice(&(names.len() as u16).to_le_bytes());
        data.extend_from_slice(&cd_size.to_le_bytes());
        data.extend_from_slice(&cd_start.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes()); // comment len

        data
    }

    #[test]
    fn lists_entries_in_central_directory_order() {
        let zip = build_test_zip(&[
            "AndroidManifest.xml",
            "classes.dex",
            "lib/arm64-v8a/libbleradar_core.so",
        ]);
        let names = entry_names(&zip).expect("parse succeeds");
        assert_eq!(
            names,
            vec![
                "AndroidManifest.xml".to_string(),
                "classes.dex".to_string(),
                "lib/arm64-v8a/libbleradar_core.so".to_string(),
            ]
        );
    }

    #[test]
    fn rejects_input_with_no_eocd() {
        assert!(matches!(
            entry_names(b"not a zip"),
            Err(ZipError::NoEndOfCentralDirectory)
        ));
    }
}
