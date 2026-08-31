//! Minimal ELF64 (little-endian) symbol table reader.
//!
//! Reproduces exactly the subset of `readelf -Ws lib.so | awk '$7!="UND" &&
//! ($4=="FUNC" || $4=="OBJECT") {print $8}' | sort -u` that
//! `tools/native_abi.sh` used: the sorted, deduplicated names of every
//! *defined* `FUNC`/`OBJECT` symbol in `.symtab` and/or `.dynsym`. It does not
//! attempt to be a general-purpose ELF parser: only the fields needed to
//! reproduce that census are read, and anything unexpected is reported as an
//! [`ElfError`] rather than guessed at.

use std::fmt;

/// A failure while parsing an ELF image.
#[derive(Debug)]
pub enum ElfError {
    /// The input is shorter than a fixed-size record it claims to contain.
    Truncated(&'static str),
    /// The file does not start with the ELF magic number.
    NotElf,
    /// The file is not a 64-bit, little-endian ELF image (the only kind the
    /// retained `aarch64` oracle library uses).
    UnsupportedFormat,
}

impl fmt::Display for ElfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ElfError::Truncated(what) => write!(f, "truncated ELF image: {what}"),
            ElfError::NotElf => write!(f, "not an ELF image (bad magic)"),
            ElfError::UnsupportedFormat => {
                write!(f, "unsupported ELF image (expected 64-bit little-endian)")
            }
        }
    }
}

impl std::error::Error for ElfError {}

const SHT_SYMTAB: u32 = 2;
const SHT_DYNSYM: u32 = 11;
const STT_OBJECT: u8 = 1;
const STT_FUNC: u8 = 2;
const SHN_UNDEF: u16 = 0;

fn u16_at(data: &[u8], off: usize, what: &'static str) -> Result<u16, ElfError> {
    let bytes: [u8; 2] = data
        .get(off..off + 2)
        .ok_or(ElfError::Truncated(what))?
        .try_into()
        .map_err(|_| ElfError::Truncated(what))?;
    Ok(u16::from_le_bytes(bytes))
}

fn u32_at(data: &[u8], off: usize, what: &'static str) -> Result<u32, ElfError> {
    let bytes: [u8; 4] = data
        .get(off..off + 4)
        .ok_or(ElfError::Truncated(what))?
        .try_into()
        .map_err(|_| ElfError::Truncated(what))?;
    Ok(u32::from_le_bytes(bytes))
}

fn u64_at(data: &[u8], off: usize, what: &'static str) -> Result<u64, ElfError> {
    let bytes: [u8; 8] = data
        .get(off..off + 8)
        .ok_or(ElfError::Truncated(what))?
        .try_into()
        .map_err(|_| ElfError::Truncated(what))?;
    Ok(u64::from_le_bytes(bytes))
}

fn c_str_at(data: &[u8], off: usize) -> Option<&str> {
    let rest = data.get(off..)?;
    let len = rest.iter().position(|&b| b == 0)?;
    std::str::from_utf8(&rest[..len]).ok()
}

/// Returns the sorted, deduplicated names of every defined (non-`UND`)
/// `FUNC`/`OBJECT` symbol in `data`'s `.symtab` and/or `.dynsym`.
pub fn defined_func_and_object_symbols(data: &[u8]) -> Result<Vec<String>, ElfError> {
    if data.len() < 20 || &data[0..4] != b"\x7fELF" {
        return Err(ElfError::NotElf);
    }
    let ei_class = data[4];
    let ei_data = data[5];
    if ei_class != 2 || ei_data != 1 {
        // ELFCLASS64 + ELFDATA2LSB only; matches the retained aarch64 oracle.
        return Err(ElfError::UnsupportedFormat);
    }

    let e_shoff = u64_at(data, 0x28, "e_shoff")? as usize;
    let e_shentsize = u16_at(data, 0x3a, "e_shentsize")? as usize;
    let e_shnum = u16_at(data, 0x3c, "e_shnum")? as usize;
    if e_shentsize < 64 {
        return Err(ElfError::Truncated("section header entry too small"));
    }

    struct Section {
        sh_type: u32,
        sh_offset: usize,
        sh_size: usize,
        sh_link: usize,
        sh_entsize: usize,
    }

    let mut sections = Vec::with_capacity(e_shnum);
    for i in 0..e_shnum {
        let base = e_shoff + i * e_shentsize;
        sections.push(Section {
            sh_type: u32_at(data, base + 4, "sh_type")?,
            sh_offset: u64_at(data, base + 24, "sh_offset")? as usize,
            sh_size: u64_at(data, base + 32, "sh_size")? as usize,
            sh_link: u32_at(data, base + 40, "sh_link")? as usize,
            sh_entsize: u64_at(data, base + 56, "sh_entsize")? as usize,
        });
    }

    let mut names: Vec<String> = Vec::new();
    for sec in &sections {
        if sec.sh_type != SHT_SYMTAB && sec.sh_type != SHT_DYNSYM {
            continue;
        }
        if sec.sh_entsize == 0 {
            continue;
        }
        let Some(strtab) = sections.get(sec.sh_link) else {
            continue;
        };
        let sym_bytes = data
            .get(sec.sh_offset..sec.sh_offset + sec.sh_size)
            .ok_or(ElfError::Truncated("symbol table"))?;
        let count = sym_bytes.len() / sec.sh_entsize;
        for i in 0..count {
            let base = i * sec.sh_entsize;
            let st_name = u32_at(sym_bytes, base, "st_name")? as usize;
            let st_info = sym_bytes[base + 4];
            let st_shndx = u16_at(sym_bytes, base + 6, "st_shndx")?;
            let sym_type = st_info & 0x0f;
            if st_shndx == SHN_UNDEF {
                continue;
            }
            if sym_type != STT_FUNC && sym_type != STT_OBJECT {
                continue;
            }
            if let Some(name) = c_str_at(data, strtab.sh_offset + st_name)
                && !name.is_empty()
            {
                names.push(name.to_string());
            }
        }
    }

    names.sort();
    names.dedup();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal single-section-header-table ELF64 LE image with one
    /// `SHT_SYMTAB` section (plus its string table) to exercise the parser
    /// without depending on the retained oracle binary.
    fn build_test_elf() -> Vec<u8> {
        let mut shstrtab = vec![0u8]; // index 0 is always the empty string.
        shstrtab.extend_from_slice(b".shstrtab\0");
        let shstrtab_name_off = 1u32;

        let mut strtab = vec![0u8];
        let undef_name_off = 0u32;
        let func_name_off = strtab.len() as u32;
        strtab.extend_from_slice(b"defined_func\0");
        let obj_name_off = strtab.len() as u32;
        strtab.extend_from_slice(b"defined_obj\0");
        let extern_name_off = strtab.len() as u32;
        strtab.extend_from_slice(b"external_func\0");

        // Symbol table: [null, UND external, defined FUNC, defined OBJECT].
        let mut symtab = Vec::new();
        let push_sym = |buf: &mut Vec<u8>, name: u32, info: u8, shndx: u16| {
            buf.extend_from_slice(&name.to_le_bytes());
            buf.push(info);
            buf.push(0); // st_other
            buf.extend_from_slice(&shndx.to_le_bytes());
            buf.extend_from_slice(&0u64.to_le_bytes()); // st_value
            buf.extend_from_slice(&0u64.to_le_bytes()); // st_size
        };
        push_sym(&mut symtab, undef_name_off, 0, 0);
        push_sym(&mut symtab, extern_name_off, STT_FUNC, 0); // UND: excluded.
        push_sym(&mut symtab, func_name_off, STT_FUNC, 1);
        push_sym(&mut symtab, obj_name_off, STT_OBJECT, 1);

        // Layout: header(64) | strtab | symtab | shstrtab, then section headers.
        let header_len = 64usize;
        let strtab_off = header_len;
        let symtab_off = strtab_off + strtab.len();
        let shstrtab_off = symtab_off + symtab.len();
        let shdr_off = shstrtab_off + shstrtab.len();

        let mut data = vec![0u8; header_len];
        data[0..4].copy_from_slice(b"\x7fELF");
        data[4] = 2; // ELFCLASS64
        data[5] = 1; // ELFDATA2LSB
        data[6] = 1; // EI_VERSION
        data[0x28..0x30].copy_from_slice(&(shdr_off as u64).to_le_bytes()); // e_shoff
        data[0x3a..0x3c].copy_from_slice(&64u16.to_le_bytes()); // e_shentsize
        data[0x3c..0x3e].copy_from_slice(&3u16.to_le_bytes()); // e_shnum
        data[0x3e..0x40].copy_from_slice(&2u16.to_le_bytes()); // e_shstrndx

        data.extend_from_slice(&strtab);
        data.extend_from_slice(&symtab);
        data.extend_from_slice(&shstrtab);

        let push_shdr = |buf: &mut Vec<u8>,
                         name: u32,
                         ty: u32,
                         off: u64,
                         size: u64,
                         link: u32,
                         entsize: u64| {
            buf.extend_from_slice(&name.to_le_bytes());
            buf.extend_from_slice(&ty.to_le_bytes());
            buf.extend_from_slice(&0u64.to_le_bytes()); // sh_flags
            buf.extend_from_slice(&0u64.to_le_bytes()); // sh_addr
            buf.extend_from_slice(&off.to_le_bytes());
            buf.extend_from_slice(&size.to_le_bytes());
            buf.extend_from_slice(&link.to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes()); // sh_info
            buf.extend_from_slice(&0u64.to_le_bytes()); // sh_addralign
            buf.extend_from_slice(&entsize.to_le_bytes());
        };
        // Section 0: NULL section (all zero is conventional; content unused).
        push_shdr(&mut data, 0, 0, 0, 0, 0, 0);
        // Section 1: .symtab, linked to section 3... but we only have 3
        // sections total (0,1,2) so link to the strtab we place at index 2.
        push_shdr(
            &mut data,
            0,
            SHT_SYMTAB,
            symtab_off as u64,
            symtab.len() as u64,
            2,
            24,
        );
        // Section 2: .strtab (referenced by section 1's sh_link).
        push_shdr(
            &mut data,
            shstrtab_name_off,
            3,
            strtab_off as u64,
            strtab.len() as u64,
            0,
            0,
        );

        data
    }

    #[test]
    fn extracts_only_defined_func_and_object_symbols() {
        let elf = build_test_elf();
        let names = defined_func_and_object_symbols(&elf).expect("parse succeeds");
        assert_eq!(
            names,
            vec!["defined_func".to_string(), "defined_obj".to_string()]
        );
    }

    #[test]
    fn rejects_non_elf_input() {
        assert!(matches!(
            defined_func_and_object_symbols(b"not an elf"),
            Err(ElfError::NotElf)
        ));
    }
}
