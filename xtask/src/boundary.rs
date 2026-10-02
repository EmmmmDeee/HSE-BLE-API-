//! `cargo xtask check-crate-boundary`: records the current dependency direction
//! inside `bleradar-core` and between it and its in-repo consumers, so the
//! staged split in `docs/CRATE_BOUNDARY_PLAN.md` can only move forward.
//!
//! Every module `crates/bleradar-core/src/lib.rs` declares is assigned one
//! [`Layer`] in [`LAYERS`]. The check reads the sources (it is textual and
//! dependency-free, like the rest of xtask; it never builds or links the
//! crate) and fails when:
//!
//! - a module is declared but unclassified, or classified but not declared;
//! - a `Radar` or `HseModel` module (or the crate root's own items) reaches an
//!   `Engine` module;
//! - a `Radar` module reaches an `HseModel` module other than through an edge
//!   recorded in [`ALLOWED_RADAR_TO_HSE_MODEL`], or a recorded edge no longer
//!   exists (the allowance is a ratchet: it only ever shrinks);
//! - what `bleradar-jni` or `bleradar-compat` imports from `bleradar_core`
//!   transitively reaches an `Engine` module;
//! - a `crate::`/`bleradar_core::` path names something the check cannot
//!   resolve to a module (it fails loudly rather than miss an edge).
//!
//! Comments and `#[cfg(test)] mod` blocks are not dependencies of the shipped
//! code and are ignored; integration tests under `bleradar-core/tests` are not
//! part of the library and are not read.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// Where a `bleradar-core` module sits relative to the BLE radar surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// The radar surface: what the Android app reaches through `bleradar-jni`,
    /// what HSE takes through `sweep`, and the radar's own scheduling and
    /// capability records. Must never reach an engine.
    Radar,
    /// The HSE entity model (entity, coordinates, tags). Must never reach an
    /// engine; the radar reaches it only through recorded edges.
    HseModel,
    /// The non-radar engines from the v0.3.0 reconstruction (evidence, OSINT,
    /// website, infrastructure, advancement, fusion, verification, pipeline and
    /// their shared validation helper). They may use radar types; nothing on
    /// the radar side may use them.
    Engine,
}

/// The pseudo-module name for items defined directly in `lib.rs`.
pub const CRATE_ROOT: &str = "(crate root)";

/// Every `bleradar-core` module and its layer. Adding a module means placing
/// it here deliberately.
pub const LAYERS: &[(&str, Layer)] = &[
    (CRATE_ROOT, Layer::Radar),
    ("adv", Layer::Radar),
    ("geo", Layer::Radar),
    ("history", Layer::Radar),
    ("identity", Layer::Radar),
    ("registry", Layer::Radar),
    ("runtime", Layer::Radar),
    ("scan", Layer::Radar),
    ("signal", Layer::Radar),
    ("sweep", Layer::Radar),
    ("tracking", Layer::Radar),
    ("update", Layer::Radar),
    ("coords", Layer::HseModel),
    ("entity", Layer::HseModel),
    ("tags", Layer::HseModel),
    ("advancement", Layer::Engine),
    ("evidence", Layer::Engine),
    ("fusion", Layer::Engine),
    ("infrastructure", Layer::Engine),
    ("osint", Layer::Engine),
    ("pipeline", Layer::Engine),
    ("validation", Layer::Engine),
    ("verification", Layer::Engine),
    ("website", Layer::Engine),
];

/// The only radar → HSE-model edges allowed today: `update` takes `Sha256`
/// and `hex_encode` from `entity` (step 1 of the plan moves them and empties
/// this list).
pub const ALLOWED_RADAR_TO_HSE_MODEL: &[(&str, &str)] = &[("update", "entity")];

/// `bleradar-core`'s library sources, relative to the repository root.
pub const CORE_SRC: &str = "crates/bleradar-core/src";
/// In-repo consumers of `bleradar-core`: a label and the directories read.
pub const CONSUMERS: &[(&str, &[&str])] = &[
    ("bleradar-jni", &["crates/bleradar-jni/src"]),
    (
        "bleradar-compat",
        &["crates/bleradar-compat/src", "crates/bleradar-compat/tests"],
    ),
];

/// What `lib.rs` declares: its modules, which module each crate-root name
/// (re-export or root item) comes from, and the root's own code.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CrateIndex {
    /// Declared modules (`mod x;` / `pub mod x;`), sorted.
    pub modules: BTreeSet<String>,
    /// Crate-root name → owning module (or [`CRATE_ROOT`]).
    pub names: BTreeMap<String, String>,
    /// `lib.rs` with comments, `mod` declarations and `pub use` statements
    /// removed: the root items' own code.
    pub root_code: String,
}

/// Removes comments (`//`, doc comments, nested `/* */`) and blanks the
/// contents of string, raw-string, byte-string and character literals, keeping
/// their delimiters and every newline. What remains is code only, so a brace,
/// a quote or a `crate::` inside a literal or a comment is never mistaken for
/// structure or for a dependency. Lifetimes (`'a`) are left as code.
pub fn mask_code(text: &str) -> String {
    let b = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let blank = |out: &mut Vec<u8>, byte: u8| out.push(if byte == b'\n' { b'\n' } else { b' ' });
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let prev_ident = i > 0 && is_ident_byte(b[i - 1]);
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            let mut depth = 0usize;
            while i < b.len() {
                if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if b[i] == b'\n' {
                        out.push(b'\n');
                    }
                    i += 1;
                }
            }
        } else if c == b'r'
            && (!prev_ident || (i > 0 && b[i - 1] == b'b' && (i < 2 || !is_ident_byte(b[i - 2]))))
            && matches!(b.get(i + 1), Some(b'"' | b'#'))
        {
            let hashes = b[i + 1..].iter().take_while(|&&x| x == b'#').count();
            if b.get(i + 1 + hashes) != Some(&b'"') {
                out.push(c);
                i += 1;
                continue;
            }
            out.push(b'r');
            out.extend(std::iter::repeat_n(b'#', hashes));
            out.push(b'"');
            i += 2 + hashes;
            while i < b.len() {
                if b[i] == b'"'
                    && b[i + 1..]
                        .iter()
                        .take(hashes)
                        .filter(|&&x| x == b'#')
                        .count()
                        == hashes
                    && b.len() >= i + 1 + hashes
                {
                    out.push(b'"');
                    out.extend(std::iter::repeat_n(b'#', hashes));
                    i += 1 + hashes;
                    break;
                }
                blank(&mut out, b[i]);
                i += 1;
            }
        } else if c == b'"' {
            out.push(b'"');
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    blank(&mut out, b[i]);
                    if let Some(&next) = b.get(i + 1) {
                        blank(&mut out, next);
                    }
                    i += 2;
                } else if b[i] == b'"' {
                    out.push(b'"');
                    i += 1;
                    break;
                } else {
                    blank(&mut out, b[i]);
                    i += 1;
                }
            }
        } else if c == b'\'' {
            // A character literal is `'x'`, `'\..'` or a multi-byte char;
            // anything else (`'a` in `&'a str`) is a lifetime.
            let close = if b.get(i + 1) == Some(&b'\\') {
                b[i + 2..]
                    .iter()
                    .take(10)
                    .position(|&x| x == b'\'')
                    .map(|p| i + 2 + p)
            } else {
                text[i + 1..]
                    .chars()
                    .next()
                    .map(|ch| i + 1 + ch.len_utf8())
                    .filter(|&end| b.get(end) == Some(&b'\''))
            };
            match close {
                Some(end) => {
                    out.push(b'\'');
                    for &byte in &b[i + 1..end] {
                        blank(&mut out, byte);
                    }
                    out.push(b'\'');
                    i = end + 1;
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    String::from_utf8(out).expect("masking only removes or blanks whole UTF-8 sequences")
}

/// Removes every `#[cfg(test)]` attribute that is followed by a `mod` item,
/// together with that module's braced body. Expects [`mask_code`] output, so
/// every brace is structural. Errors on an unbalanced body rather than
/// silently keeping or dropping code.
pub fn strip_test_modules(code: &str) -> Result<String, String> {
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    while let Some(at) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "#[cfg(test)]".len()..];
        let trimmed = after.trim_start();
        let is_mod = trimmed.starts_with("mod ") || trimmed.starts_with("pub mod ");
        if !is_mod {
            out.push_str("#[cfg(test)]");
            rest = after;
            continue;
        }
        let open = after
            .find('{')
            .ok_or("a #[cfg(test)] mod has no body".to_string())?;
        let end = matching_brace(after, open)?;
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The byte index of the `}` closing the `{` at `open` in masked code.
fn matching_brace(code: &str, open: usize) -> Result<usize, String> {
    let mut depth = 0usize;
    for (i, byte) in code.bytes().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }
    }
    Err("unbalanced braces".to_string())
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Every identifier inside a `{ ... }` use-group (nested groups included),
/// except the local alias after `as` (`x as y` reaches `x`, not `y`). A
/// wildcard anywhere in the group is an error: it could bring in any name,
/// so it cannot be resolved to a known set of modules.
fn group_idents(group: &str) -> Result<Vec<String>, String> {
    if group.contains('*') {
        return Err("a wildcard (`*`) import cannot be resolved to modules".to_string());
    }
    let mut found = Vec::new();
    let mut skip_alias = false;
    for token in group
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|s| !s.is_empty() && !s.as_bytes()[0].is_ascii_digit())
    {
        if skip_alias {
            skip_alias = false;
            continue;
        }
        match token {
            "as" => skip_alias = true,
            "self" | "super" | "crate" => {}
            _ => found.push(token.to_string()),
        }
    }
    Ok(found)
}

/// The names reached through `prefix` paths in `code` (`prefix` is e.g.
/// `"crate::"` or `"bleradar_core::"`): `prefix::name` yields `name`, and
/// `prefix::{a, b::c}` yields every identifier in the group. A match preceded
/// by an identifier character (`my_crate::`) is not a match. A wildcard
/// (`prefix::*`, `prefix::m::*`, or `*` inside a group) is an error, never a
/// silent "no edge": it can expose any name for unqualified use.
pub fn path_targets(code: &str, prefix: &str) -> Result<Vec<String>, String> {
    let bytes = code.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(rel) = code[from..].find(prefix) {
        let at = from + rel;
        let start = at + prefix.len();
        from = start;
        if at > 0 && is_ident_byte(bytes[at - 1]) {
            continue;
        }
        if bytes.get(start) == Some(&b'{') {
            let end = matching_brace(code, start)
                .map_err(|_| format!("unbalanced `{prefix}{{` group"))?;
            found.extend(
                group_idents(&code[start + 1..end])
                    .map_err(|e| format!("`{prefix}{{..}}`: {e}"))?,
            );
            from = end;
        } else {
            let len = bytes[start..]
                .iter()
                .take_while(|&&b| is_ident_byte(b))
                .count();
            // Follow the rest of the path (`a::b::c`) to catch a trailing
            // wildcard (`a::*`) or a nested group (`a::{b, *}`).
            let mut end = start + len;
            while code[end..].starts_with("::") {
                let next = end + 2;
                match bytes.get(next) {
                    Some(b'*') => {
                        return Err(format!(
                            "`{prefix}{}*`: a wildcard (`*`) import cannot be resolved to modules",
                            &code[start..next]
                        ));
                    }
                    Some(b'{') => {
                        let close = matching_brace(code, next)
                            .map_err(|_| format!("unbalanced `{prefix}..::{{` group"))?;
                        group_idents(&code[next + 1..close])
                            .map_err(|e| format!("`{prefix}{}{{..}}`: {e}", &code[start..next]))?;
                        end = close + 1;
                        break;
                    }
                    _ => {
                        let more = bytes[next..]
                            .iter()
                            .take_while(|&&b| is_ident_byte(b))
                            .count();
                        if more == 0 {
                            break;
                        }
                        end = next + more;
                    }
                }
            }
            if len > 0 {
                found.push(code[start..start + len].to_string());
            } else if bytes.get(start) == Some(&b'*') {
                return Err(format!(
                    "`{prefix}*`: a wildcard (`*`) import cannot be resolved to modules"
                ));
            }
            from = end.max(start);
        }
    }
    Ok(found)
}

/// Reads `lib.rs`: declared modules, the module behind every crate-root name
/// (`pub use m::{..}` / `pub use m::x` re-exports, `X as Y` renames, and items
/// defined in the root itself), and the root's own code. Any visibility
/// (`pub`, `pub(crate)`, `pub(super)`, `pub(in ..)`) is accepted on a `mod`
/// declaration; a `#[path]` attribute or a wildcard re-export is an error,
/// because either would hide a module or a name from the check.
pub fn parse_lib(lib_rs: &str) -> Result<CrateIndex, String> {
    let code = mask_code(lib_rs);
    let mut index = CrateIndex::default();
    let mut root_code = String::new();
    let mut statement = String::new();
    for line in code.lines() {
        let t = line.trim();
        if statement.is_empty() {
            if t.starts_with("#[path") {
                return Err(format!(
                    "lib.rs: `{t}` relocates a module file; not supported"
                ));
            }
            let decl = strip_visibility(t);
            if let Some(name) = decl.strip_prefix("mod ").and_then(|r| r.strip_suffix(';')) {
                index.modules.insert(name.trim().to_string());
                continue;
            }
            if !(t.starts_with("pub use ") || t.starts_with("use ")) {
                root_code.push_str(line);
                root_code.push('\n');
                if line.starts_with("pub ") || line.starts_with("pub(") {
                    let words: Vec<&str> = line.split_whitespace().collect();
                    if let Some(pos) = words.iter().position(|w| {
                        matches!(
                            *w,
                            "fn" | "struct" | "enum" | "const" | "static" | "type" | "trait"
                        )
                    }) && let Some(name) = words.get(pos + 1)
                    {
                        let name: String = name
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect();
                        index.names.insert(name, CRATE_ROOT.to_string());
                    }
                }
                continue;
            }
        }
        statement.push_str(t);
        statement.push(' ');
        if !t.ends_with(';') {
            continue;
        }
        let body = statement
            .trim()
            .trim_start_matches("pub ")
            .trim_start_matches("use ")
            .trim_end_matches(';')
            .to_string();
        statement.clear();
        let Some((module, items)) = body.split_once("::") else {
            continue;
        };
        let module = module.trim().to_string();
        let items = items.trim();
        let entries: Vec<&str> = match items.strip_prefix('{').and_then(|i| i.strip_suffix('}')) {
            Some(group) => group.split(',').collect(),
            None => vec![items],
        };
        for entry in entries {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            if entry.contains('*') {
                return Err(format!(
                    "lib.rs: `use {module}::{entry}` is a wildcard re-export; its names cannot be resolved"
                ));
            }
            let exported = entry.rsplit(" as ").next().unwrap_or(entry).trim();
            let exported = exported.rsplit("::").next().unwrap_or(exported);
            index.names.insert(exported.to_string(), module.clone());
        }
    }
    let modules = index.modules.clone();
    index
        .names
        .retain(|_, module| module == CRATE_ROOT || modules.contains(module));
    index.root_code = root_code;
    Ok(index)
}

/// `t` without a leading visibility qualifier (`pub`, `pub(crate)`,
/// `pub(super)`, `pub(self)`, `pub(in path)`).
fn strip_visibility(t: &str) -> &str {
    if let Some(rest) = t.strip_prefix("pub(") {
        return rest
            .split_once(')')
            .map_or(t, |(_, after)| after.trim_start());
    }
    t.strip_prefix("pub ").map_or(t, str::trim_start)
}

/// Resolves a name reached through a `crate::`/`bleradar_core::` path to the
/// module that owns it, or `None` when it names nothing the index knows.
pub fn resolve<'a>(index: &'a CrateIndex, name: &'a str) -> Option<&'a str> {
    if index.modules.contains(name) {
        Some(name)
    } else {
        index.names.get(name).map(String::as_str)
    }
}

/// The modules `code` (one module's source) depends on through `crate::` and
/// `super::` paths, excluding `own`. Unresolvable names are errors.
pub fn module_edges(index: &CrateIndex, own: &str, code: &str) -> Result<BTreeSet<String>, String> {
    let code = strip_test_modules(&mask_code(code)).map_err(|e| format!("{own}: {e}"))?;
    let mut edges = BTreeSet::new();
    for prefix in ["crate::", "super::"] {
        for name in path_targets(&code, prefix).map_err(|e| format!("{own}: {e}"))? {
            match resolve(index, &name) {
                Some(module) if module != own => {
                    edges.insert(module.to_string());
                }
                Some(_) => {}
                None => return Err(format!("{own}: cannot resolve `{prefix}{name}`")),
            }
        }
    }
    Ok(edges)
}

/// The modules a consumer's sources import from `bleradar_core`.
pub fn consumer_modules(
    index: &CrateIndex,
    label: &str,
    code: &str,
) -> Result<BTreeSet<String>, String> {
    let code = strip_test_modules(&mask_code(code)).map_err(|e| format!("{label}: {e}"))?;
    let mut modules = BTreeSet::new();
    for name in path_targets(&code, "bleradar_core::").map_err(|e| format!("{label}: {e}"))? {
        let module = resolve(index, &name)
            .ok_or_else(|| format!("{label}: cannot resolve `bleradar_core::{name}`"))?;
        modules.insert(module.to_string());
    }
    Ok(modules)
}

/// Every module reachable from `start` along `edges` (`start` included).
pub fn closure(
    edges: &BTreeMap<String, BTreeSet<String>>,
    start: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut seen = start.clone();
    let mut stack: Vec<String> = start.iter().cloned().collect();
    while let Some(module) = stack.pop() {
        for next in edges.get(&module).into_iter().flatten() {
            if seen.insert(next.clone()) {
                stack.push(next.clone());
            }
        }
    }
    seen
}

/// The layer of `module`, if classified.
pub fn layer_of(layers: &[(&str, Layer)], module: &str) -> Option<Layer> {
    layers.iter().find(|(m, _)| *m == module).map(|(_, l)| *l)
}

/// Applies every rule to an already-computed graph. Pure, so each rule is
/// unit-testable with fixtures. Returns every violation found.
pub fn violations(
    modules: &BTreeSet<String>,
    edges: &BTreeMap<String, BTreeSet<String>>,
    consumers: &BTreeMap<String, BTreeSet<String>>,
    layers: &[(&str, Layer)],
    allowed_radar_to_model: &[(&str, &str)],
) -> Vec<String> {
    let mut found = Vec::new();
    for module in modules {
        if layer_of(layers, module).is_none() {
            found.push(format!(
                "module `{module}` is declared in lib.rs but has no layer in LAYERS"
            ));
        }
    }
    for (module, _) in layers {
        if *module != CRATE_ROOT && !modules.contains(*module) {
            found.push(format!(
                "LAYERS classifies `{module}`, which lib.rs does not declare"
            ));
        }
    }
    for (from, targets) in edges {
        let Some(from_layer) = layer_of(layers, from) else {
            continue;
        };
        for to in targets {
            let Some(to_layer) = layer_of(layers, to) else {
                continue;
            };
            match (from_layer, to_layer) {
                (Layer::Radar | Layer::HseModel, Layer::Engine) => found.push(format!(
                    "`{from}` ({from_layer:?}) depends on engine `{to}`: the radar side must never reach an engine"
                )),
                (Layer::Radar, Layer::HseModel)
                    if !allowed_radar_to_model.contains(&(from.as_str(), to.as_str())) =>
                {
                    found.push(format!(
                        "`{from}` (Radar) depends on `{to}` (HseModel), an edge not recorded in ALLOWED_RADAR_TO_HSE_MODEL"
                    ))
                }
                _ => {}
            }
        }
    }
    for (from, to) in allowed_radar_to_model {
        let present = edges.get(*from).is_some_and(|t| t.contains(*to));
        if !present {
            found.push(format!(
                "ALLOWED_RADAR_TO_HSE_MODEL records `{from}` -> `{to}`, which no longer exists: remove it so it cannot come back"
            ));
        }
    }
    for (label, direct) in consumers {
        let reached = closure(edges, direct);
        for module in &reached {
            if layer_of(layers, module) == Some(Layer::Engine) {
                found.push(format!(
                    "{label} reaches engine `{module}` through bleradar_core"
                ));
            }
        }
    }
    found
}

/// Reads every `.rs` file under `dir` (recursively), sorted by path.
fn read_rust_sources(dir: &Path) -> Result<Vec<(String, String)>, String> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = fs::read_dir(&d).map_err(|e| format!("listing {}: {e}", d.display()))?;
        for entry in entries {
            let path = entry
                .map_err(|e| format!("listing {}: {e}", d.display()))?
                .path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x == "rs") {
                let text = fs::read_to_string(&path)
                    .map_err(|e| format!("reading {}: {e}", path.display()))?;
                files.push((path.display().to_string(), text));
            }
        }
    }
    files.sort();
    Ok(files)
}

/// The dependency graph as found in the tree at `root`.
pub struct Graph {
    /// Declared modules.
    pub modules: BTreeSet<String>,
    /// Module (or [`CRATE_ROOT`]) → modules it depends on.
    pub edges: BTreeMap<String, BTreeSet<String>>,
    /// Consumer crate → `bleradar-core` modules it imports directly.
    pub consumers: BTreeMap<String, BTreeSet<String>>,
}

/// Builds the graph from the sources under `root`.
pub fn graph_at(root: &Path) -> Result<Graph, String> {
    let src = root.join(CORE_SRC);
    let lib = fs::read_to_string(src.join("lib.rs")).map_err(|e| format!("reading lib.rs: {e}"))?;
    let index = parse_lib(&lib)?;
    let mut edges = BTreeMap::new();
    edges.insert(CRATE_ROOT.to_string(), {
        let mut root_edges = module_edges(&index, CRATE_ROOT, &index.root_code)?;
        // Root items may also name a module directly (`osint::Foo`).
        let code = strip_test_modules(&index.root_code)?;
        for module in &index.modules {
            let needle = format!("{module}::");
            let hit = code
                .match_indices(&needle)
                .any(|(at, _)| at == 0 || !is_ident_byte(code.as_bytes()[at - 1]));
            if hit {
                root_edges.insert(module.clone());
            }
        }
        root_edges
    });
    for module in &index.modules {
        let path = src.join(format!("{module}.rs"));
        let text =
            fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
        edges.insert(module.clone(), module_edges(&index, module, &text)?);
    }
    let mut consumers = BTreeMap::new();
    for (label, dirs) in CONSUMERS {
        let mut direct = BTreeSet::new();
        for dir in *dirs {
            for (path, text) in read_rust_sources(&root.join(dir))? {
                direct.extend(consumer_modules(&index, &path, &text)?);
            }
        }
        consumers.insert((*label).to_string(), direct);
    }
    Ok(Graph {
        modules: index.modules,
        edges,
        consumers,
    })
}

/// `cargo xtask check-crate-boundary`.
pub fn cmd_check_crate_boundary(root: &Path) -> Result<(), String> {
    let graph = graph_at(root)?;
    let found = violations(
        &graph.modules,
        &graph.edges,
        &graph.consumers,
        LAYERS,
        ALLOWED_RADAR_TO_HSE_MODEL,
    );
    for (module, targets) in &graph.edges {
        let layer =
            layer_of(LAYERS, module).map_or("unclassified".to_string(), |l| format!("{l:?}"));
        let targets: Vec<&str> = targets.iter().map(String::as_str).collect();
        println!(
            "  {module} [{layer}] -> {}",
            if targets.is_empty() {
                "(none)".to_string()
            } else {
                targets.join(", ")
            }
        );
    }
    for (label, direct) in &graph.consumers {
        let reached: Vec<String> = closure(&graph.edges, direct).into_iter().collect();
        println!("  {label} reaches: {}", reached.join(", "));
    }
    if found.is_empty() {
        println!(
            "Crate boundary holds: no radar or HSE-model module reaches an engine; radar -> HSE-model edges are exactly {:?}; bleradar-jni and bleradar-compat reach no engine.",
            ALLOWED_RADAR_TO_HSE_MODEL
        );
        Ok(())
    } else {
        println!("Crate boundary violated (docs/CRATE_BOUNDARY_PLAN.md):");
        for violation in &found {
            println!("  - {violation}");
        }
        Err("crate boundary violated".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIB: &str = "\
//! Doc mentioning crate::osint, which is not a dependency.
mod osint;
mod signal;
pub mod entity;
mod update;

pub use osint::{OsintEngine, SearchError as OsintError};
pub use signal::{
    Confidence, ble_distance_m,
};
pub use update::Updater;

/// A root item.
pub fn wifi_band(mhz: u16) -> u16 {
    mhz
}
";

    const FIXTURE_LAYERS: &[(&str, Layer)] = &[
        (CRATE_ROOT, Layer::Radar),
        ("osint", Layer::Engine),
        ("signal", Layer::Radar),
        ("entity", Layer::HseModel),
        ("update", Layer::Radar),
    ];

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    fn graph(pairs: &[(&str, &[&str])]) -> BTreeMap<String, BTreeSet<String>> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), set(v)))
            .collect()
    }

    #[test]
    fn mask_code_removes_comments_and_blanks_literals_but_keeps_lifetimes() {
        let src = "let a = \"crate::osint {\"; // crate::osint\n/* crate::x /* nested */ */ let c = '{';\nfn f<'a>(x: &'a str) {}\nlet r = r#\"crate::osint \"}\"#; let b = br\"}\";";
        let masked = mask_code(src);
        assert!(!masked.contains("crate::"), "{masked}");
        assert_eq!(masked.matches('{').count(), 1, "{masked}");
        assert_eq!(masked.matches('}').count(), 1, "{masked}");
        assert!(masked.contains("fn f<'a>(x: &'a str) {}"), "{masked}");
        assert_eq!(masked.lines().count(), src.lines().count());
    }

    #[test]
    fn strip_test_modules_drops_only_cfg_test_mod_bodies() {
        let code = "use crate::signal;\n#[cfg(test)]\nfn helper() { crate::osint::x(); }\n#[cfg(test)]\nmod tests { fn t() { if x { crate::osint::y(); } } }\nfn after() {}";
        let stripped = strip_test_modules(&mask_code(code)).unwrap();
        assert!(stripped.contains("crate::signal"));
        assert!(
            stripped.contains("fn helper() { crate::osint::x(); }"),
            "a cfg(test) fn is not a mod"
        );
        assert!(!stripped.contains("crate::osint::y"));
        assert!(stripped.contains("fn after() {}"));
        assert!(strip_test_modules("#[cfg(test)]\nmod t { {").is_err());
    }

    #[test]
    fn path_targets_reads_plain_paths_and_nested_groups_only_at_word_starts() {
        let code = "use crate::{Confidence, geo::{LatLon, bearing as b}, self as me};\nlet x = crate::signal::f(); my_crate::nope();";
        assert_eq!(
            path_targets(code, "crate::").unwrap(),
            vec!["Confidence", "geo", "LatLon", "bearing", "signal"]
        );
    }

    #[test]
    fn wildcard_imports_are_errors_never_a_silent_no_edge() {
        for code in [
            "use crate::*;",
            "use super::*;",
            "use crate::osint::*;",
            "use crate::{signal, osint::*};",
            "use crate::osint::{Foo, *};",
        ] {
            let prefix = if code.contains("super") {
                "super::"
            } else {
                "crate::"
            };
            let err = path_targets(code, prefix).unwrap_err();
            assert!(err.contains("wildcard"), "{code}: {err}");
        }
        assert!(path_targets("use bleradar_core::*;", "bleradar_core::").is_err());
        // A multiplication after a path is not an import.
        assert_eq!(
            path_targets("let x = crate::K * 2;", "crate::").unwrap(),
            vec!["K"]
        );
        assert!(
            parse_lib("mod osint;\npub use osint::*;\n")
                .unwrap_err()
                .contains("wildcard")
        );
    }

    #[test]
    fn parse_lib_reads_every_visibility_on_mod_and_rejects_path_attributes() {
        let index = parse_lib(
            "mod a;\npub mod b;\npub(crate) mod c;\npub(super) mod d;\npub(in crate) mod e;\n",
        )
        .unwrap();
        assert_eq!(index.modules, set(&["a", "b", "c", "d", "e"]));
        assert!(parse_lib("#[path = \"x.rs\"]\nmod a;\n").is_err());
    }

    #[test]
    fn parse_lib_maps_modules_reexports_renames_and_root_items() {
        let index = parse_lib(LIB).unwrap();
        assert_eq!(index.modules, set(&["entity", "osint", "signal", "update"]));
        assert_eq!(resolve(&index, "OsintEngine"), Some("osint"));
        assert_eq!(resolve(&index, "OsintError"), Some("osint"));
        assert_eq!(resolve(&index, "SearchError"), None, "renamed away");
        assert_eq!(resolve(&index, "ble_distance_m"), Some("signal"));
        assert_eq!(resolve(&index, "Updater"), Some("update"));
        assert_eq!(resolve(&index, "wifi_band"), Some(CRATE_ROOT));
        assert_eq!(resolve(&index, "signal"), Some("signal"));
        assert!(
            !index.root_code.contains("osint"),
            "doc comments and pub use are not root code"
        );
    }

    #[test]
    fn module_edges_resolve_names_and_fail_loudly_on_unknown_paths() {
        let index = parse_lib(LIB).unwrap();
        let code = "use crate::{Confidence, entity};\nfn f() { super::OsintEngine::new(); crate::update::x(); }\n#[cfg(test)]\nmod tests { use crate::Updater; }";
        assert_eq!(
            module_edges(&index, "update", code).unwrap(),
            set(&["entity", "osint", "signal"])
        );
        assert_eq!(
            module_edges(&index, "signal", "fn f() { crate::Mystery::x(); }"),
            Err("signal: cannot resolve `crate::Mystery`".to_string())
        );
        assert_eq!(
            consumer_modules(
                &index,
                "jni",
                "use bleradar_core::{Confidence, update::Updater};"
            )
            .unwrap(),
            set(&["signal", "update"])
        );
        assert!(consumer_modules(&index, "jni", "bleradar_core::nothing()").is_err());
    }

    #[test]
    fn violations_pass_for_a_clean_graph() {
        let edges = graph(&[
            (CRATE_ROOT, &[]),
            ("signal", &[]),
            ("update", &["entity", "signal"]),
            ("entity", &[]),
            ("osint", &["signal", "entity"]),
        ]);
        let consumers = graph(&[("jni", &["update"])]);
        let modules = set(&["entity", "osint", "signal", "update"]);
        assert_eq!(
            violations(
                &modules,
                &edges,
                &consumers,
                FIXTURE_LAYERS,
                &[("update", "entity")]
            ),
            Vec::<String>::new()
        );
    }

    #[test]
    fn violations_catch_radar_and_model_reaching_an_engine_and_consumers_reaching_one_transitively()
    {
        let edges = graph(&[
            ("signal", &["osint"]),
            ("entity", &["osint"]),
            ("update", &["entity"]),
            ("osint", &[]),
        ]);
        let consumers = graph(&[("jni", &["update"])]);
        let modules = set(&["entity", "osint", "signal", "update"]);
        assert_eq!(
            violations(&modules, &edges, &consumers, FIXTURE_LAYERS, &[("update", "entity")]),
            vec![
                "`entity` (HseModel) depends on engine `osint`: the radar side must never reach an engine".to_string(),
                "`signal` (Radar) depends on engine `osint`: the radar side must never reach an engine".to_string(),
                "jni reaches engine `osint` through bleradar_core".to_string(),
            ]
        );
    }

    #[test]
    fn violations_hold_the_radar_to_model_allowance_as_a_ratchet() {
        let modules = set(&["entity", "osint", "signal", "update"]);
        let consumers = BTreeMap::new();
        let new_edge = graph(&[("signal", &["entity"]), ("update", &["entity"])]);
        assert_eq!(
            violations(&modules, &new_edge, &consumers, FIXTURE_LAYERS, &[("update", "entity")]),
            vec!["`signal` (Radar) depends on `entity` (HseModel), an edge not recorded in ALLOWED_RADAR_TO_HSE_MODEL".to_string()]
        );
        let edge_gone = graph(&[("update", &[])]);
        assert_eq!(
            violations(&modules, &edge_gone, &consumers, FIXTURE_LAYERS, &[("update", "entity")]),
            vec!["ALLOWED_RADAR_TO_HSE_MODEL records `update` -> `entity`, which no longer exists: remove it so it cannot come back".to_string()]
        );
    }

    #[test]
    fn violations_require_every_module_classified_and_every_classification_declared() {
        let modules = set(&["entity", "osint", "signal", "update", "brand_new"]);
        let layers: Vec<(&str, Layer)> = FIXTURE_LAYERS
            .iter()
            .copied()
            .chain([("removed", Layer::Engine)])
            .collect();
        let edges = graph(&[("update", &["entity"])]);
        assert_eq!(
            violations(
                &modules,
                &edges,
                &BTreeMap::new(),
                &layers,
                &[("update", "entity")]
            ),
            vec![
                "module `brand_new` is declared in lib.rs but has no layer in LAYERS".to_string(),
                "LAYERS classifies `removed`, which lib.rs does not declare".to_string(),
            ]
        );
    }

    #[test]
    fn the_committed_tree_holds_the_recorded_boundary() {
        let root = crate::repo_root_from(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))
            .expect("repo root");
        let graph = graph_at(&root).expect("the committed sources parse");
        assert_eq!(
            violations(
                &graph.modules,
                &graph.edges,
                &graph.consumers,
                LAYERS,
                ALLOWED_RADAR_TO_HSE_MODEL
            ),
            Vec::<String>::new()
        );
        // The recorded facts the plan's later steps start from.
        assert_eq!(graph.edges["update"], set(&["entity"]));
        let jni = closure(&graph.edges, &graph.consumers["bleradar-jni"]);
        assert!(
            jni.contains("entity") && jni.contains("coords") && jni.contains("tags"),
            "{jni:?}"
        );
        for engine in LAYERS.iter().filter(|(_, l)| *l == Layer::Engine) {
            assert!(!jni.contains(engine.0), "{jni:?}");
        }
    }
}
