//! Dependency-free inventory and parity tools for the v0.3.0 migration snapshot.
//!
//! These replace `tools/apk_inventory.py`, `tools/native_abi.sh`, and
//! `tools/parity_report.py`. Nothing here links a third-party crate.

pub mod elf;
pub mod parity;
pub mod sha256;
pub mod zip;
