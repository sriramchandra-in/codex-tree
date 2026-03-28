use std::collections::HashSet;
use std::path::Path;

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::error::{ParserError, Result};
use crate::registry::ParserRegistry;
use crate::types::{Export, ModuleIndex, TreeEntry, TreeStructure, Visibility};

// ── Public API ────────────────────────────────────────────────────────────────

/// Parse a single source file and return a `ModuleIndex`.
///
/// Steps:
///   1. Resolve the file extension and look up a registered `LanguageAdapter`.
///   2. Build a `tree_sitter::Parser`, set the grammar, and parse the bytes.
///   3. Delegate symbol and import extraction to the adapter.
///   4. Derive the public `exports` list from symbols with public visibility.
///   5. Hash the raw bytes for change-detection.
pub fn parse_file(path: &Path, source: &[u8], registry: &ParserRegistry) -> Result<ModuleIndex> {
    // ── 1. Extension lookup ──────────────────────────────────────────────────
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let adapter = registry
        .get_adapter(ext)
        .ok_or_else(|| ParserError::UnsupportedLanguage(ext.to_string()))?;

    // ── 2. Parse ─────────────────────────────────────────────────────────────
    let mut ts_parser = tree_sitter::Parser::new();
    ts_parser
        .set_language(&adapter.tree_sitter_language())
        .map_err(|e| ParserError::TreeSitterError(e.to_string()))?;

    let tree = ts_parser
        .parse(source, None)
        .ok_or_else(|| ParserError::TreeSitterError("tree-sitter returned None".to_string()))?;

    // ── 3. Symbol / import extraction ────────────────────────────────────────
    let symbols = adapter.extract_symbols(&tree, source);
    let imports = adapter.extract_imports(&tree, source);

    // ── 4. Exports from public symbols ────────────────────────────────────────
    let exports: Vec<Export> = symbols
        .iter()
        .filter(|s| {
            matches!(s.visibility, Visibility::Public | Visibility::PublicCrate)
        })
        .map(|s| Export {
            name: s.name.clone(),
            kind: s.kind.clone(),
        })
        .collect();

    // ── 5. Content hash ──────────────────────────────────────────────────────
    let content_hash = compute_hash(source);

    // ── 6. Normalise the stored path to forward slashes ──────────────────────
    let path_str = path
        .to_string_lossy()
        .replace('\\', "/");

    Ok(ModuleIndex {
        path: path_str,
        language: adapter.language_id().to_string(),
        content_hash,
        symbols,
        imports,
        exports,
    })
}

/// Walk a directory tree and parse every supported source file.
///
/// Returns both a `TreeStructure` (the hierarchy for `tree.json`) and a flat
/// `Vec<ModuleIndex>` (one entry per successfully parsed file).
///
/// Files whose extension has no registered adapter are silently skipped.
/// Directories listed in `exclude_dirs` (matched by directory name, not full
/// path) are not entered.
pub fn parse_directory(
    root: &Path,
    registry: &ParserRegistry,
    exclude_dirs: &[&str],
) -> Result<(TreeStructure, Vec<ModuleIndex>)> {
    let mut modules: Vec<ModuleIndex> = Vec::new();

    // ── Walk the tree ────────────────────────────────────────────────────────
    for entry in WalkDir::new(root).sort_by_file_name() {
        let entry = entry.map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?;

        // Skip excluded directories.
        if entry.file_type().is_dir() {
            if let Some(dir_name) = entry.file_name().to_str() {
                if exclude_dirs.contains(&dir_name) {
                    // Tell walkdir not to descend by using filter_entry would be
                    // cleaner, but we can't easily prune here in a plain for-loop.
                    // Instead we check on every path component below.
                }
            }
            continue;
        }

        // Check that no path component is an excluded directory.
        let rel = match entry.path().strip_prefix(root) {
            Ok(r) => r,
            Err(_) => continue,
        };

        if rel.components().any(|c| {
            if let std::path::Component::Normal(n) = c {
                if let Some(s) = n.to_str() {
                    return exclude_dirs.contains(&s);
                }
            }
            false
        }) {
            continue;
        }

        // Skip non-files.
        if !entry.file_type().is_file() {
            continue;
        }

        // Read source and attempt to parse.
        let source = match std::fs::read(entry.path()) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let rel_str = rel.to_string_lossy().replace('\\', "/");

        match parse_file(Path::new(&rel_str), &source, registry) {
            Ok(mut module) => {
                // Ensure the path stored is the repo-relative one.
                module.path = rel_str;
                modules.push(module);
            }
            Err(ParserError::UnsupportedLanguage(_)) => {
                // Silently skip files we can't parse.
            }
            Err(e) => return Err(e),
        }
    }

    // ── Build TreeStructure ──────────────────────────────────────────────────
    let tree = build_tree_structure(root, &modules);

    Ok((tree, modules))
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// SHA-256 content hash, returned as `"sha256:<hex_lowercase>"`.
fn compute_hash(source: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source);
    let result = hasher.finalize();
    format!("sha256:{:x}", result)
}

/// Convert a flat list of `ModuleIndex` entries into a `TreeStructure` that
/// contains both `TreeEntry::File` entries and aggregate `TreeEntry::Directory`
/// entries.
fn build_tree_structure(root: &Path, modules: &[ModuleIndex]) -> TreeStructure {
    // Collect every directory that contains at least one parsed file.
    let mut dir_set: HashSet<String> = HashSet::new();

    for module in modules {
        let p = Path::new(&module.path);
        let mut current = p.parent();
        while let Some(dir) = current {
            let dir_str = dir.to_string_lossy().replace('\\', "/");
            if dir_str.is_empty() {
                break;
            }
            dir_set.insert(dir_str);
            current = dir.parent();
        }
    }

    // ── File entries ──────────────────────────────────────────────────────────
    let mut entries: Vec<TreeEntry> = modules
        .iter()
        .map(|m| {
            // Count lines by counting newlines in the source on disk; we don't
            // re-read the file here — use symbol span heuristic instead.
            // A simpler proxy: end line of the last symbol, or 0 if no symbols.
            let line_count = m
                .symbols
                .iter()
                .map(|s| s.span.end_line)
                .max()
                .unwrap_or(0);

            TreeEntry::File {
                path: m.path.clone(),
                language: m.language.clone(),
                symbol_count: m.symbols.len(),
                line_count,
                imports_count: m.imports.len(),
                exports_count: m.exports.len(),
            }
        })
        .collect();

    // ── Directory entries ─────────────────────────────────────────────────────
    for dir in &dir_set {
        // Direct children (files or sub-directories) of this directory.
        let direct_children_count = modules
            .iter()
            .filter(|m| {
                let p = Path::new(&m.path);
                p.parent()
                    .map(|par| par.to_string_lossy().replace('\\', "/") == *dir)
                    .unwrap_or(false)
            })
            .count()
            + dir_set
                .iter()
                .filter(|sub| {
                    Path::new(sub)
                        .parent()
                        .map(|par| par.to_string_lossy().replace('\\', "/") == *dir)
                        .unwrap_or(false)
                })
                .count();

        // Aggregate totals across *all* files anywhere under this directory.
        let (total_symbol_count, total_line_count) = modules
            .iter()
            .filter(|m| {
                m.path.starts_with(&format!("{}/", dir)) || m.path == *dir
            })
            .fold((0usize, 0usize), |(syms, lines), m| {
                let file_lines = m
                    .symbols
                    .iter()
                    .map(|s| s.span.end_line)
                    .max()
                    .unwrap_or(0);
                (syms + m.symbols.len(), lines + file_lines)
            });

        entries.push(TreeEntry::Directory {
            path: dir.clone(),
            children_count: direct_children_count,
            total_symbol_count,
            total_line_count,
        });
    }

    // Sort alphabetically so the output is deterministic.
    entries.sort_by(|a, b| {
        let path_a = entry_path(a);
        let path_b = entry_path(b);
        path_a.cmp(path_b)
    });

    TreeStructure {
        root: root.to_string_lossy().to_string(),
        entries,
    }
}

/// Extract the `path` field from any `TreeEntry` variant.
fn entry_path(entry: &TreeEntry) -> &str {
    match entry {
        TreeEntry::File { path, .. } => path,
        TreeEntry::Directory { path, .. } => path,
    }
}
