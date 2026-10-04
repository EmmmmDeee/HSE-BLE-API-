//! `cargo xtask verify-apk-rebuild`: the committed APK must equal a fresh
//! build once the APK Signing Block is removed from both.
//!
//! `build-apk` signs with a debug key generated in each build directory, so
//! the signed file never reproduces across machines. The APK Signing Block
//! (signature scheme v2/v3, which `apksigner` inserts between the last entry
//! and the central directory) carries the signer's certificate and
//! signatures. Removing it, and moving the end-of-central-directory (EOCD)
//! record's central-directory offset back by its length, leaves the package
//! exactly as `apksigner` laid it out, minus the signature. That is the
//! *stripped* form, and its SHA-256 is the stripped digest.
//!
//! Two stripped packages are compared in one of two ways:
//! - **Layout-independent (the default, the CI gate):** every entry's bytes
//!   are compared, entries matched by name. That covers its local header,
//!   name, extra field and alignment padding, its compressed data and
//!   anything up to the next entry. So is its central-directory record
//!   (minus the local-header offset), the central directory's size and entry
//!   count, and the rest of the EOCD. Only the order of the entries, and the
//!   offsets that follow from it, may differ. `build-apk` stores the
//!   entries `zip -r` finds in directory-read order, which differs between
//!   filesystems (ext4 hashes names with a per-filesystem seed). On the
//!   committed APK and a rebuild on another host, every entry was
//!   byte-identical and only that order differed.
//! - **Byte-identical (`--byte-identical`):** the stripped digests must be equal.
//!
//! Both digests are always printed. Neither comparison reads or needs a key,
//! and both hold whatever key signed either file.
//!
//! The comparison fails closed. These are all refused rather than compared:
//! - a file that is not a well-formed single-disk ZIP;
//! - an archive comment, which APKs do not carry and which could hide a
//!   second EOCD;
//! - ZIP64;
//! - data before the first entry or between the central directory and the
//!   EOCD;
//! - a duplicate entry name;
//! - a local header that disagrees with its central-directory record;
//! - a file with no APK Signing Block, or one with no v2/v3 signature in it
//!   (an unsigned or v1-only package is not what `build-apk` produces).

use crate::sha256;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const ZIP64_EOCD_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
const CENTRAL_DIR_SIGNATURE: u32 = 0x0201_4b50;
const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const EOCD_LEN: usize = 22;
const CENTRAL_DIR_FIXED_LEN: usize = 46;
const LOCAL_HEADER_FIXED_LEN: usize = 30;
/// The APK Signing Block's trailing magic, immediately before the central directory.
pub const SIGNING_BLOCK_MAGIC: &[u8; 16] = b"APK Sig Block 42";
/// Signature-scheme IDs in the block that make it a signature (v2, v3, v3.1).
/// Other IDs, such as the verity padding `0x42726577`, may appear beside them.
const SIGNATURE_SCHEME_IDS: [(u32, &str); 3] = [
    (0x7109_871a, "v2"),
    (0xf053_68c0, "v3"),
    (0x1b93_ad61, "v3.1"),
];

/// One entry of a parsed package, in central-directory order.
#[derive(Debug)]
struct EntrySpan {
    name: String,
    name_bytes: Vec<u8>,
    /// In [`Stripped::bytes`]: the local header, name, extra field, data (and
    /// any data descriptor), up to the next entry's local header or, for the
    /// last entry, to where the signing block was.
    local: Range<usize>,
    /// In [`Stripped::bytes`]: the central-directory record.
    central: Range<usize>,
}

/// A signed package reduced to what reproduces across signers.
#[derive(Debug)]
pub struct Stripped {
    /// The package without its signing block: entries, central directory, and
    /// the EOCD with its central-directory offset pointing where the block began.
    pub bytes: Vec<u8>,
    /// Offset and length of the removed APK Signing Block.
    pub block_offset: usize,
    pub block_len: usize,
    /// The signature schemes the block carried (`v2`, `v3`, `v3.1`).
    pub schemes: Vec<&'static str>,
    entries: Vec<EntrySpan>,
    /// Length of the EOCD at the end of [`Stripped::bytes`] (22: a comment is refused).
    eocd_tail_len: usize,
    data_len: usize,
}

impl Stripped {
    /// SHA-256 of [`Stripped::bytes`].
    pub fn digest_hex(&self) -> String {
        sha256::to_hex(&sha256::sha256(&self.bytes))
    }

    /// The layout-independent form: entries sorted by name, each as its
    /// length-prefixed span and its central-directory record with the
    /// local-header offset zeroed, then the EOCD with its central-directory
    /// offset zeroed. Every byte of [`Stripped::bytes`] is in it except those offsets.
    fn canonical(&self) -> Vec<u8> {
        let mut sorted: Vec<&EntrySpan> = self.entries.iter().collect();
        sorted.sort_by(|a, b| a.name_bytes.cmp(&b.name_bytes));
        let mut out = Vec::with_capacity(self.bytes.len() + 8 * sorted.len());
        for entry in sorted {
            let span = &self.bytes[entry.local.clone()];
            out.extend_from_slice(&(span.len() as u64).to_le_bytes());
            out.extend_from_slice(span);
            out.extend_from_slice(&central_record_without_offset(&self.bytes, &entry.central));
        }
        let eocd = self.bytes.len() - self.eocd_tail_len;
        out.extend_from_slice(&self.bytes[eocd..eocd + 16]);
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&self.bytes[eocd + 20..]);
        out
    }

    /// SHA-256 of the layout-independent form.
    pub fn layout_independent_digest_hex(&self) -> String {
        sha256::to_hex(&sha256::sha256(&self.canonical()))
    }
}

/// A central-directory record with its local-header offset (bytes 42..46) zeroed.
fn central_record_without_offset(data: &[u8], range: &Range<usize>) -> Vec<u8> {
    let mut record = data[range.clone()].to_vec();
    record[42..46].fill(0);
    record
}

fn u16_at(data: &[u8], at: usize) -> Result<u16, String> {
    data.get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| format!("truncated: a 2-byte field at offset {at} lies past the end"))
}

fn u32_at(data: &[u8], at: usize) -> Result<u32, String> {
    data.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| format!("truncated: a 4-byte field at offset {at} lies past the end"))
}

fn u64_at(data: &[u8], at: usize) -> Result<u64, String> {
    data.get(at..at + 8)
        .map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
        .ok_or_else(|| format!("truncated: an 8-byte field at offset {at} lies past the end"))
}

/// The EOCD's offset. It must be the last 22 bytes: an archive comment is
/// refused, and so is a file with no EOCD at all.
fn find_eocd(data: &[u8]) -> Result<usize, String> {
    if data.len() < EOCD_LEN {
        return Err(format!(
            "not a ZIP: {} bytes, shorter than an end-of-central-directory record",
            data.len()
        ));
    }
    let at = data.len() - EOCD_LEN;
    if u32_at(data, at)? == EOCD_SIGNATURE {
        return Ok(at);
    }
    // Tell a commented archive from a damaged one.
    let floor = data.len().saturating_sub(EOCD_LEN + 0xffff);
    for i in (floor..at).rev() {
        if u32_at(data, i)? == EOCD_SIGNATURE
            && i + EOCD_LEN + usize::from(u16_at(data, i + 20)?) == data.len()
        {
            return Err(format!(
                "the archive has a {}-byte comment; refused (an APK carries none, and a comment can hide a second end-of-central-directory record)",
                data.len() - i - EOCD_LEN
            ));
        }
    }
    Err(
        "no end-of-central-directory record at the end of the file (truncated or not a ZIP)"
            .to_string(),
    )
}

/// Parses a signed package and removes its APK Signing Block. Fails closed
/// on anything it does not fully understand (see the module docs).
pub fn strip_signing_block(data: &[u8]) -> Result<Stripped, String> {
    let eocd = find_eocd(data)?;
    let disk = u16_at(data, eocd + 4)?;
    let cd_disk = u16_at(data, eocd + 6)?;
    let entries_on_disk = u16_at(data, eocd + 8)?;
    let entries_total = u16_at(data, eocd + 10)?;
    let cd_size = u32_at(data, eocd + 12)?;
    let cd_offset = u32_at(data, eocd + 16)?;
    if entries_on_disk == 0xffff
        || entries_total == 0xffff
        || cd_size == 0xffff_ffff
        || cd_offset == 0xffff_ffff
        || (eocd >= 20 && u32_at(data, eocd - 20)? == ZIP64_EOCD_LOCATOR_SIGNATURE)
    {
        return Err(
            "ZIP64 archive; refused (not produced by build-apk and not handled here)".to_string(),
        );
    }
    if disk != 0 || cd_disk != 0 || entries_on_disk != entries_total {
        return Err("multi-disk (spanned) archive; refused".to_string());
    }
    if entries_total == 0 {
        return Err("the archive has no entries".to_string());
    }
    let cd_offset = cd_offset as usize;
    let cd_end = cd_offset
        .checked_add(cd_size as usize)
        .ok_or("central directory size overflows")?;
    if cd_end != eocd {
        return Err(format!(
            "the central directory ({cd_offset}..{cd_end}) does not end where the end-of-central-directory record starts ({eocd})"
        ));
    }

    // The APK Signing Block: [u64 size][id-value pairs][u64 size][magic], ending at the central directory.
    if cd_offset < 32 || data.get(cd_offset - 16..cd_offset) != Some(&SIGNING_BLOCK_MAGIC[..]) {
        return Err("no APK Signing Block before the central directory (unsigned, or signed with v1 only); refused: both packages must carry a v2/v3 signature".to_string());
    }
    let block_size = u64_at(data, cd_offset - 24)?;
    let block_len = usize::try_from(block_size)
        .ok()
        .and_then(|s| s.checked_add(8))
        .filter(|&len| block_size >= 24 && len <= cd_offset)
        .ok_or_else(|| format!("APK Signing Block size {block_size} does not fit before the central directory at {cd_offset}"))?;
    let block_offset = cd_offset - block_len;
    if u64_at(data, block_offset)? != block_size {
        return Err(format!(
            "APK Signing Block at {block_offset}: its leading size does not equal its trailing size {block_size}"
        ));
    }
    let mut schemes = Vec::new();
    let mut pair = block_offset + 8;
    let pairs_end = cd_offset - 24;
    while pair < pairs_end {
        let len = u64_at(data, pair)?;
        let len = usize::try_from(len)
            .ok()
            .filter(|&len| len >= 4 && len <= pairs_end - pair - 8)
            .ok_or_else(|| format!("APK Signing Block: an ID-value pair at {pair} has length {len}, which does not fit"))?;
        let id = u32_at(data, pair + 8)?;
        if let Some((_, scheme)) = SIGNATURE_SCHEME_IDS.iter().find(|(known, _)| *known == id) {
            schemes.push(*scheme);
        }
        pair += 8 + len;
    }
    if pair != pairs_end {
        return Err("APK Signing Block: its ID-value pairs overrun the block".to_string());
    }
    if schemes.is_empty() {
        return Err("the APK Signing Block carries no v2/v3 signature; refused".to_string());
    }

    // The central directory, record by record, each checked against its local header.
    let mut entries = Vec::with_capacity(usize::from(entries_total));
    let mut names = BTreeSet::new();
    let mut at = cd_offset;
    for index in 0..entries_total {
        if u32_at(data, at)? != CENTRAL_DIR_SIGNATURE {
            return Err(format!(
                "central directory record {index} at {at}: bad signature"
            ));
        }
        let flags = u16_at(data, at + 8)?;
        let method = u16_at(data, at + 10)?;
        let compressed = u32_at(data, at + 20)?;
        let uncompressed = u32_at(data, at + 24)?;
        let name_len = usize::from(u16_at(data, at + 28)?);
        let extra_len = usize::from(u16_at(data, at + 30)?);
        let comment_len = usize::from(u16_at(data, at + 32)?);
        let start_disk = u16_at(data, at + 34)?;
        let local = u32_at(data, at + 42)?;
        let end = at + CENTRAL_DIR_FIXED_LEN + name_len + extra_len + comment_len;
        if end > cd_end {
            return Err(format!(
                "central directory record {index} runs past the central directory"
            ));
        }
        let name_bytes = &data[at + CENTRAL_DIR_FIXED_LEN..at + CENTRAL_DIR_FIXED_LEN + name_len];
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        if compressed == 0xffff_ffff || uncompressed == 0xffff_ffff || local == 0xffff_ffff {
            return Err(format!("{name}: ZIP64 sizes or offset; refused"));
        }
        if start_disk != 0 {
            return Err(format!("{name}: starts on disk {start_disk}; refused"));
        }
        if !names.insert(name_bytes.to_vec()) {
            return Err(format!(
                "{name}: the name appears twice; refused (ZIP readers resolve a duplicate differently)"
            ));
        }
        let local = local as usize;
        if local >= block_offset {
            return Err(format!(
                "{name}: local header offset {local} is not before the APK Signing Block ({block_offset})"
            ));
        }
        if u32_at(data, local)? != LOCAL_HEADER_SIGNATURE {
            return Err(format!("{name}: no local file header at offset {local}"));
        }
        let local_flags = u16_at(data, local + 6)?;
        let local_method = u16_at(data, local + 8)?;
        let local_name_len = usize::from(u16_at(data, local + 26)?);
        let local_extra_len = usize::from(u16_at(data, local + 28)?);
        let local_name = data
            .get(local + LOCAL_HEADER_FIXED_LEN..local + LOCAL_HEADER_FIXED_LEN + local_name_len)
            .ok_or_else(|| format!("{name}: local header name runs past the end"))?;
        if local_name != name_bytes || local_flags != flags || local_method != method {
            return Err(format!(
                "{name}: the local header disagrees with the central directory (name, flags or method)"
            ));
        }
        let data_end =
            local + LOCAL_HEADER_FIXED_LEN + local_name_len + local_extra_len + compressed as usize;
        entries.push((
            local,
            data_end,
            EntrySpan {
                name,
                name_bytes: name_bytes.to_vec(),
                local: local..local,
                central: at..end,
            },
        ));
        at = end;
    }
    if at != cd_end {
        return Err(format!(
            "the central directory holds {} bytes after its {entries_total} records",
            cd_end - at
        ));
    }

    // Each entry's span runs from its local header to the next one (or to the signing block).
    // Together they must tile the region before the block: the first entry at offset 0, and no
    // entry's data running into the next. Bytes past an entry's data (a data descriptor, or
    // anything else) stay inside its span, so they are compared too.
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by_key(|&i| entries[i].0);
    if entries[order[0]].0 != 0 {
        return Err(format!(
            "{}: the first local header is at {}, so {} bytes before it are unaccounted for",
            entries[order[0]].2.name, entries[order[0]].0, entries[order[0]].0
        ));
    }
    for (k, &i) in order.iter().enumerate() {
        let span_end = order
            .get(k + 1)
            .map_or(block_offset, |&next| entries[next].0);
        let (local, data_end) = (entries[i].0, entries[i].1);
        if data_end > span_end {
            return Err(format!(
                "{}: its data ({local}..{data_end}) runs into the next entry or the signing block at {span_end}",
                entries[i].2.name
            ));
        }
        entries[i].2.local = local..span_end;
    }
    // Ranges into the stripped bytes: entries keep their offsets, the central directory moves back.
    let entries: Vec<EntrySpan> = entries
        .into_iter()
        .map(|(_, _, mut e)| {
            e.central = e.central.start - block_len..e.central.end - block_len;
            e
        })
        .collect();

    let mut bytes = Vec::with_capacity(data.len() - block_len);
    bytes.extend_from_slice(&data[..block_offset]);
    bytes.extend_from_slice(&data[cd_offset..eocd + 16]);
    bytes.extend_from_slice(&(block_offset as u32).to_le_bytes());
    bytes.extend_from_slice(&data[eocd + 20..]);
    Ok(Stripped {
        bytes,
        block_offset,
        block_len,
        schemes,
        entries,
        eocd_tail_len: EOCD_LEN,
        data_len: data.len(),
    })
}

/// How two stripped packages must agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Every entry's bytes, matched by name, the central directory and the EOCD;
    /// entry order (and the offsets it implies) may differ.
    LayoutIndependent,
    /// The stripped files must be identical.
    ByteIdentical,
}

/// The first entry where two stripped packages differ, and how; `None` if
/// every entry is identical. `LayoutIndependent` matches entries by name, in
/// name order; `ByteIdentical` compares them position by position.
fn first_difference(c: &Stripped, r: &Stripped, mode: Mode) -> Option<String> {
    let compare = |ce: &EntrySpan, re: &EntrySpan| {
        if c.bytes[ce.local.clone()] != r.bytes[re.local.clone()] {
            Some(format!("{} (local header, data or padding)", ce.name))
        } else if central_record_without_offset(&c.bytes, &ce.central)
            != central_record_without_offset(&r.bytes, &re.central)
        {
            Some(format!("{} (central directory record)", ce.name))
        } else {
            None
        }
    };
    let only = |entry: Option<&EntrySpan>, other: Option<&EntrySpan>| match (entry, other) {
        (Some(ce), None) => Some(format!("{} (only in the committed APK)", ce.name)),
        (None, Some(re)) => Some(format!("{} (only in the rebuilt APK)", re.name)),
        _ => None,
    };
    match mode {
        Mode::LayoutIndependent => {
            let by_name = |s: &Stripped| -> BTreeMap<Vec<u8>, usize> {
                s.entries
                    .iter()
                    .enumerate()
                    .map(|(i, e)| (e.name_bytes.clone(), i))
                    .collect()
            };
            let (cn, rn) = (by_name(c), by_name(r));
            let names: BTreeSet<&Vec<u8>> = cn.keys().chain(rn.keys()).collect();
            names.into_iter().find_map(|name| {
                let ce = cn.get(name).map(|&i| &c.entries[i]);
                let re = rn.get(name).map(|&i| &r.entries[i]);
                match (ce, re) {
                    (Some(ce), Some(re)) => compare(ce, re),
                    _ => only(ce, re),
                }
            })
        }
        Mode::ByteIdentical => {
            let count = c.entries.len().max(r.entries.len());
            (0..count).find_map(|i| match (c.entries.get(i), r.entries.get(i)) {
                (Some(ce), Some(re)) if ce.name_bytes != re.name_bytes => Some(format!(
                    "entry {i}: {} in the committed APK, {} in the rebuilt APK",
                    ce.name, re.name
                )),
                (Some(ce), Some(re)) => compare(ce, re),
                (ce, re) => only(ce, re),
            })
        }
    }
}

/// The entry names in stored order, for a report.
fn stored_order(s: &Stripped) -> String {
    let names: Vec<&str> = s.entries.iter().map(|e| e.name.as_str()).collect();
    names.join(", ")
}

/// Compares two signed packages with their signing blocks removed. `Ok` is a
/// summary; `Err` names both packages' digests and the first differing entry.
pub fn verify(
    committed_label: &str,
    committed: &[u8],
    rebuilt_label: &str,
    rebuilt: &[u8],
    mode: Mode,
) -> Result<String, String> {
    let c = strip_signing_block(committed).map_err(|e| format!("{committed_label}: {e}"))?;
    let r = strip_signing_block(rebuilt).map_err(|e| format!("{rebuilt_label}: {e}"))?;
    let describe = |label: &str, s: &Stripped| {
        format!(
            "{label}: {} bytes, {} entries, APK Signing Block {} bytes at {} ({}) removed\n    stripped sha256 {}\n    layout-independent sha256 {}",
            s.data_len,
            s.entries.len(),
            s.block_len,
            s.block_offset,
            s.schemes.join("+"),
            s.digest_hex(),
            s.layout_independent_digest_hex()
        )
    };
    let report = format!(
        "  {}\n  {}",
        describe(committed_label, &c),
        describe(rebuilt_label, &r)
    );
    let byte_identical = c.bytes == r.bytes;
    let equal = match mode {
        Mode::ByteIdentical => byte_identical,
        Mode::LayoutIndependent => c.canonical() == r.canonical(),
    };
    if equal {
        let how = if byte_identical {
            "byte-identical, entry order included".to_string()
        } else {
            format!(
                "every entry byte-identical; only the entry order differs (committed: {}; rebuilt: {})",
                stored_order(&c),
                stored_order(&r)
            )
        };
        return Ok(format!(
            "the rebuilt APK matches the committed APK with the APK Signing Block removed from both: {how}\n{report}"
        ));
    }
    let first = first_difference(&c, &r, mode).unwrap_or_else(|| match mode {
        Mode::ByteIdentical if c.canonical() == r.canonical() => format!(
            "none: every entry is byte-identical and only the entry order differs (committed: {}; rebuilt: {})",
            stored_order(&c),
            stored_order(&r)
        ),
        _ => "none: every entry is identical, so the central directory's size or entry count, or the end record, differs".to_string(),
    });
    let what = match mode {
        Mode::ByteIdentical => "is not byte-identical to",
        Mode::LayoutIndependent => "does not match",
    };
    Err(format!(
        "the rebuilt APK {what} the committed APK with the APK Signing Block removed from both:\n{report}\n  first difference: {first}\nrun `cargo xtask build-apk` and commit its output, or find what made the build differ"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored (uncompressed) ZIP of `entries`, with `block` inserted between the
    /// entries and the central directory, as `apksigner` places the APK Signing Block.
    fn zip(entries: &[(&str, &[u8])], block: Option<&[u8]>) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, content) in entries {
            let offset = out.len() as u32;
            let crc = crc32(content);
            let header = |sig: u32, central: bool| {
                let mut h = Vec::new();
                h.extend_from_slice(&sig.to_le_bytes());
                if central {
                    h.extend_from_slice(&20u16.to_le_bytes()); // version made by
                }
                h.extend_from_slice(&20u16.to_le_bytes()); // version needed
                h.extend_from_slice(&0u16.to_le_bytes()); // flags
                h.extend_from_slice(&0u16.to_le_bytes()); // method: stored
                h.extend_from_slice(&0u16.to_le_bytes()); // time
                h.extend_from_slice(&0x21u16.to_le_bytes()); // date: 1980-01-01
                h.extend_from_slice(&crc.to_le_bytes());
                h.extend_from_slice(&(content.len() as u32).to_le_bytes());
                h.extend_from_slice(&(content.len() as u32).to_le_bytes());
                h.extend_from_slice(&(name.len() as u16).to_le_bytes());
                h.extend_from_slice(&0u16.to_le_bytes()); // extra
                if central {
                    h.extend_from_slice(&0u16.to_le_bytes()); // comment
                    h.extend_from_slice(&0u16.to_le_bytes()); // disk
                    h.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
                    h.extend_from_slice(&0u32.to_le_bytes()); // external attributes
                    h.extend_from_slice(&offset.to_le_bytes());
                }
                h.extend_from_slice(name.as_bytes());
                h
            };
            out.extend(header(LOCAL_HEADER_SIGNATURE, false));
            out.extend_from_slice(content);
            central.extend(header(CENTRAL_DIR_SIGNATURE, true));
        }
        if let Some(block) = block {
            out.extend_from_slice(block);
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&EOCD_SIGNATURE.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    /// An APK Signing Block holding `pairs` (ID, value), framed as the format defines.
    fn block(pairs: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (id, value) in pairs {
            body.extend_from_slice(&((value.len() + 4) as u64).to_le_bytes());
            body.extend_from_slice(&id.to_le_bytes());
            body.extend_from_slice(value);
        }
        let size = (body.len() + 8 + 16) as u64;
        let mut out = size.to_le_bytes().to_vec();
        out.extend(body);
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(SIGNING_BLOCK_MAGIC);
        out
    }

    /// Two different "signers": the block's contents and lengths differ.
    fn signer_a() -> Vec<u8> {
        block(&[
            (0x7109_871a, vec![0xa1; 300]),
            (0xf053_68c0, vec![0xa2; 300]),
        ])
    }
    fn signer_b() -> Vec<u8> {
        block(&[(0x7109_871a, vec![0xb1; 517]), (0x4272_6577, vec![0; 3000])])
    }

    const ENTRIES: &[(&str, &[u8])] = &[
        ("AndroidManifest.xml", b"<manifest/>"),
        ("classes.dex", b"dex\n035\0 synthetic"),
        ("lib/arm64-v8a/libbleradar_jni.so", b"\x7fELF synthetic"),
    ];

    #[test]
    fn the_same_contents_under_different_signing_blocks_match() {
        let a = zip(ENTRIES, Some(&signer_a()));
        let b = zip(ENTRIES, Some(&signer_b()));
        assert_ne!(a, b, "the fixtures must differ as whole files");
        let summary = verify("committed", &a, "rebuilt", &b, Mode::LayoutIndependent)
            .expect("same contents must match");
        assert!(summary.contains("3 entries"), "{summary}");
        assert!(
            summary.contains("v2+v3") && summary.contains("(v2)"),
            "{summary}"
        );
    }

    #[test]
    fn the_stripped_form_is_the_package_without_its_block() {
        let unsigned = zip(ENTRIES, None);
        let signed = zip(ENTRIES, Some(&signer_a()));
        let stripped = strip_signing_block(&signed).unwrap();
        assert_eq!(stripped.bytes, unsigned);
        // The block sat where the unsigned package's central directory starts.
        assert_eq!(
            stripped.block_offset as u32,
            u32_at(&unsigned, unsigned.len() - 6).unwrap()
        );
        assert_eq!(stripped.block_len, signer_a().len());
    }

    #[test]
    fn a_changed_entry_fails_naming_it_and_both_digests() {
        let a = zip(ENTRIES, Some(&signer_a()));
        let mut changed = ENTRIES.to_vec();
        changed[1] = ("classes.dex", b"dex\n035\0 synthetiC");
        let b = zip(&changed, Some(&signer_b()));
        let err = verify("committed", &a, "rebuilt", &b, Mode::LayoutIndependent).unwrap_err();
        assert!(
            err.contains("first difference: classes.dex (local header, data or padding)"),
            "{err}"
        );
        let digests = [&a, &b].map(|f| strip_signing_block(f).unwrap().digest_hex());
        assert_ne!(digests[0], digests[1]);
        assert!(
            err.contains(&digests[0]) && err.contains(&digests[1]),
            "{err}"
        );
    }

    #[test]
    fn an_added_entry_fails_naming_it() {
        let a = zip(ENTRIES, Some(&signer_a()));
        let mut added = ENTRIES.to_vec();
        added.push(("assets/extra.txt", b"extra"));
        let b = zip(&added, Some(&signer_a()));
        let err = verify("committed", &a, "rebuilt", &b, Mode::LayoutIndependent).unwrap_err();
        assert!(
            err.contains("first difference: assets/extra.txt (only in the rebuilt APK)"),
            "{err}"
        );
        let err = verify("committed", &b, "rebuilt", &a, Mode::LayoutIndependent).unwrap_err();
        assert!(
            err.contains("assets/extra.txt (only in the committed APK)"),
            "{err}"
        );
    }

    #[test]
    fn reordered_entries_pass_layout_independent_and_fail_byte_identical() {
        let a = zip(ENTRIES, Some(&signer_a()));
        let mut reordered = ENTRIES.to_vec();
        reordered.swap(0, 2);
        let b = zip(&reordered, Some(&signer_b()));
        let ok = verify("committed", &a, "rebuilt", &b, Mode::LayoutIndependent).unwrap();
        assert!(ok.contains("only the entry order differs"), "{ok}");
        let err = verify("committed", &a, "rebuilt", &b, Mode::ByteIdentical).unwrap_err();
        assert!(
            err.contains("entry 0: AndroidManifest.xml in the committed APK, lib/arm64-v8a/libbleradar_jni.so in the rebuilt APK"),
            "{err}"
        );
        let same = verify(
            "committed",
            &a,
            "rebuilt",
            &zip(ENTRIES, Some(&signer_b())),
            Mode::ByteIdentical,
        )
        .unwrap();
        assert!(
            same.contains("byte-identical, entry order included"),
            "{same}"
        );
    }

    #[test]
    fn every_tamper_fails_in_both_modes() {
        let a = zip(ENTRIES, Some(&signer_a()));
        let mut changed = ENTRIES.to_vec();
        changed[2] = ("lib/arm64-v8a/libbleradar_jni.so", b"\x7fELF synthetiC");
        let mut added = ENTRIES.to_vec();
        added.push(("assets/extra.txt", b"extra"));
        let removed = &ENTRIES[..2];
        for mode in [Mode::LayoutIndependent, Mode::ByteIdentical] {
            for (what, other) in [
                ("changed", zip(&changed, Some(&signer_b()))),
                ("added", zip(&added, Some(&signer_b()))),
                ("removed", zip(removed, Some(&signer_b()))),
            ] {
                let err = verify("committed", &a, "rebuilt", &other, mode).unwrap_err();
                assert!(err.contains("first difference: "), "{mode:?} {what}: {err}");
            }
            // A central-directory-only change (external attributes, e.g. a file mode).
            let mut attrs = a.clone();
            let cd = u32_at(&attrs, attrs.len() - 6).unwrap() as usize;
            attrs[cd + 40] ^= 0x01;
            let err = verify("committed", &a, "rebuilt", &attrs, mode).unwrap_err();
            assert!(
                err.contains("AndroidManifest.xml (central directory record)"),
                "{mode:?}: {err}"
            );
        }
    }

    #[test]
    fn truncated_and_corrupt_archives_fail() {
        let good = zip(ENTRIES, Some(&signer_a()));
        for cut in [1, 21, 22, good.len() / 2, good.len() - 1] {
            let truncated = &good[..good.len() - cut];
            assert!(
                verify(
                    "committed",
                    &good,
                    "rebuilt",
                    truncated,
                    Mode::LayoutIndependent
                )
                .is_err(),
                "cut {cut} bytes"
            );
            assert!(
                verify(
                    "committed",
                    truncated,
                    "rebuilt",
                    &good,
                    Mode::LayoutIndependent
                )
                .is_err(),
                "cut {cut} bytes"
            );
        }
        assert!(
            verify("committed", &good, "rebuilt", &[], Mode::LayoutIndependent)
                .unwrap_err()
                .contains("not a ZIP")
        );
        // A central directory record's signature, a local header's signature, the EOCD's CD size.
        let cd_offset = u32_at(&good, good.len() - 6).unwrap() as usize;
        for (at, what) in [
            (cd_offset, "bad signature"),
            (0, "no local file header"),
            (good.len() - 10, "does not end where"),
        ] {
            let mut corrupt = good.clone();
            corrupt[at] ^= 0xff;
            let err = verify(
                "committed",
                &good,
                "rebuilt",
                &corrupt,
                Mode::LayoutIndependent,
            )
            .unwrap_err();
            assert!(
                err.starts_with("rebuilt: ") && err.contains(what),
                "{what}: {err}"
            );
        }
    }

    #[test]
    fn a_missing_signing_block_fails_in_either_file() {
        let signed = zip(ENTRIES, Some(&signer_a()));
        let unsigned = zip(ENTRIES, None);
        let err = verify(
            "committed",
            &unsigned,
            "rebuilt",
            &signed,
            Mode::LayoutIndependent,
        )
        .unwrap_err();
        assert!(err.starts_with("committed: no APK Signing Block"), "{err}");
        let err = verify(
            "committed",
            &signed,
            "rebuilt",
            &unsigned,
            Mode::LayoutIndependent,
        )
        .unwrap_err();
        assert!(err.starts_with("rebuilt: no APK Signing Block"), "{err}");
        assert!(
            verify(
                "committed",
                &unsigned,
                "rebuilt",
                &unsigned,
                Mode::LayoutIndependent
            )
            .is_err()
        );
    }

    #[test]
    fn a_signing_block_without_a_v2_or_v3_signature_fails() {
        let padding_only = zip(ENTRIES, Some(&block(&[(0x4272_6577, vec![0; 64])])));
        let err = strip_signing_block(&padding_only).unwrap_err();
        assert!(err.contains("no v2/v3 signature"), "{err}");
        let mut bad_size = signer_a();
        bad_size[0] ^= 1; // leading size no longer equals the trailing one
        assert!(
            strip_signing_block(&zip(ENTRIES, Some(&bad_size)))
                .unwrap_err()
                .contains("leading size")
        );
    }

    #[test]
    fn a_comment_zip64_and_multi_disk_archives_are_refused() {
        let good = zip(ENTRIES, Some(&signer_a()));
        let eocd = good.len() - EOCD_LEN;
        let mut commented = good.clone();
        commented[eocd + 20..eocd + 22].copy_from_slice(&5u16.to_le_bytes());
        commented.extend_from_slice(b"hello");
        assert!(
            strip_signing_block(&commented)
                .unwrap_err()
                .contains("5-byte comment")
        );
        let mut zip64 = good.clone();
        zip64[eocd + 16..eocd + 20].copy_from_slice(&0xffff_ffffu32.to_le_bytes());
        assert!(strip_signing_block(&zip64).unwrap_err().contains("ZIP64"));
        let mut spanned = good.clone();
        spanned[eocd + 4] = 1;
        assert!(
            strip_signing_block(&spanned)
                .unwrap_err()
                .contains("multi-disk")
        );
    }

    #[test]
    fn duplicate_names_and_bytes_before_the_first_entry_are_refused() {
        let dup = zip(&[ENTRIES[0], ENTRIES[0]], Some(&signer_a()));
        assert!(
            strip_signing_block(&dup)
                .unwrap_err()
                .contains("appears twice")
        );
        // Prepend 4 bytes and shift every recorded offset: the first entry no longer starts at 0.
        let good = zip(ENTRIES, Some(&signer_a()));
        let mut prefixed = b"JUNK".to_vec();
        prefixed.extend_from_slice(&good);
        let eocd = prefixed.len() - EOCD_LEN;
        let cd = u32_at(&prefixed, eocd + 16).unwrap() + 4;
        prefixed[eocd + 16..eocd + 20].copy_from_slice(&cd.to_le_bytes());
        let mut at = cd as usize;
        for _ in 0..ENTRIES.len() {
            let local = u32_at(&prefixed, at + 42).unwrap() + 4;
            prefixed[at + 42..at + 46].copy_from_slice(&local.to_le_bytes());
            at += CENTRAL_DIR_FIXED_LEN + usize::from(u16_at(&prefixed, at + 28).unwrap());
        }
        assert!(
            strip_signing_block(&prefixed)
                .unwrap_err()
                .contains("unaccounted for")
        );
    }
}
