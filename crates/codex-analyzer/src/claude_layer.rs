/// Claude optimization layer generator — locally builds L1/L2/L3 markdown files.
///
/// # What this module does
///
/// The `.codex-tree/claude/` directory holds three progressively detailed
/// markdown summaries of the repository:
///
/// | Level | Tokens  | Use case                            |
/// |-------|---------|-------------------------------------|
/// | L1    | ~500    | Quick tasks, Haiku-class models     |
/// | L2    | ~2,000  | Implementation work, Sonnet-class   |
/// | L3    | full    | Architecture decisions, Opus-class  |
///
/// **No API calls.** This is pure data transformation: tree + intent → markdown.
/// In Scala terms, every public function here is a pure `def` with no side effects
/// except `write_claude_layer` (analogous to a Scala `IO` action).
///
/// # String building pattern
///
/// Rust's idiom for building strings is to import `std::fmt::Write` (the trait
/// that adds `.write_fmt` / `write!` / `writeln!` to `String`) and use
/// `write!(buf, "...")` macros. This avoids repeated allocation from `+` on
/// `String` (like Scala's `.mkString` on a `StringBuilder` vs string concat).
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::Path;

use codex_parser::types::{ModuleIndex, SymbolKind, TreeEntry, TreeStructure, TreeVersion, Visibility};

use crate::types::IntentOutput;

// ── Metadata header ───────────────────────────────────────────────────────────

/// Returns the standard metadata comment that starts every generated file.
///
/// Example output:
/// ```text
/// <!-- codex-tree: generation=3, delta_count=7, format_version=1.0.0 -->
/// ```
///
/// In Scala: `def generateMetadataHeader(version: TreeVersion): String`
pub fn generate_metadata_header(version: &TreeVersion) -> String {
    format!(
        "<!-- codex-tree: generation={}, delta_count={}, format_version={} -->",
        version.tree_version.generation,
        version.tree_version.delta_count,
        version.format_version,
    )
}

// ── L1 — project summary (~500 tokens) ───────────────────────────────────────

/// Generates the L1 markdown: a concise project summary (~500 tokens).
///
/// Intended for Haiku-class models or any context where only the broad
/// shape of the project matters — entry points, key public types, quick stats.
///
/// In Scala: `def generateL1(tree: TreeStructure, modules: Seq[ModuleIndex], version: TreeVersion): String`
pub fn generate_l1(
    tree: &TreeStructure,
    modules: &[ModuleIndex],
    version: &TreeVersion,
) -> String {
    let mut buf = String::new();

    // Metadata header
    writeln!(buf, "{}", generate_metadata_header(version)).unwrap();
    writeln!(buf).unwrap();

    // Project overview
    let project_name = project_name_from_root(&tree.root);
    writeln!(buf, "# Project Overview").unwrap();
    writeln!(buf).unwrap();
    writeln!(
        buf,
        "**{}** — generation {}, {} incremental updates",
        project_name,
        version.tree_version.generation,
        version.tree_version.delta_count,
    )
    .unwrap();
    writeln!(buf).unwrap();

    // Quick stats
    writeln!(buf, "---").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "## Quick Stats").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "| Metric | Value |").unwrap();
    writeln!(buf, "|--------|-------|").unwrap();
    writeln!(buf, "| Files | {} |", version.stats.total_files).unwrap();
    writeln!(buf, "| Symbols | {} |", version.stats.total_symbols).unwrap();
    writeln!(buf, "| Lines of code | {} |", version.stats.total_lines_of_code).unwrap();
    writeln!(buf, "| Languages | {} |", version.stats.languages.join(", ")).unwrap();
    writeln!(buf).unwrap();

    // Architecture — top-level directories (depth 1 only)
    writeln!(buf, "---").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "## Architecture").unwrap();
    writeln!(buf).unwrap();
    for entry in top_level_dirs(tree) {
        if let TreeEntry::Directory { path, children_count, total_symbol_count, .. } = entry {
            writeln!(
                buf,
                "- **{}** — {} children, {} symbols",
                path, children_count, total_symbol_count,
            )
            .unwrap();
        }
    }
    writeln!(buf).unwrap();

    // Entry points
    writeln!(buf, "---").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "## Entry Points").unwrap();
    writeln!(buf).unwrap();
    let entry_points = find_entry_points(modules);
    if entry_points.is_empty() {
        writeln!(buf, "_No `main` functions detected._").unwrap();
    } else {
        for path in &entry_points {
            writeln!(buf, "- `{}`", path).unwrap();
        }
    }
    writeln!(buf).unwrap();

    // Key public types — up to 10, preferring doc-commented ones
    writeln!(buf, "---").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "## Key Public Types").unwrap();
    writeln!(buf).unwrap();
    let key_types = collect_key_public_types(modules, 10);
    if key_types.is_empty() {
        writeln!(buf, "_No public structs, traits, or enums found._").unwrap();
    } else {
        for (module_path, sig, doc) in &key_types {
            if let Some(comment) = doc {
                // First sentence of the doc comment — stop at '.' or newline
                let summary = first_sentence(comment);
                writeln!(buf, "- `{}` _(in `{}`)_ — {}", sig, module_path, summary).unwrap();
            } else {
                writeln!(buf, "- `{}` _(in `{}`)_", sig, module_path).unwrap();
            }
        }
    }

    buf
}

// ── L2 — module detail (~2000 tokens) ────────────────────────────────────────

/// Generates the L2 markdown: module-level detail (~2,000 tokens).
///
/// Extends L1 with per-module symbol lists, the dependency graph, and
/// any design patterns from the intent layer.
///
/// In Scala: `def generateL2(tree: TreeStructure, modules: Seq[ModuleIndex], version: TreeVersion, intent: Option[IntentOutput]): String`
pub fn generate_l2(
    tree: &TreeStructure,
    modules: &[ModuleIndex],
    version: &TreeVersion,
    intent: Option<&IntentOutput>,
) -> String {
    // Start with the full L1 content as a base
    let mut buf = generate_l1(tree, modules, version);

    // Module details
    writeln!(buf).unwrap();
    writeln!(buf, "---").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "## Module Details").unwrap();
    writeln!(buf).unwrap();
    for module in modules {
        let public_symbols: Vec<_> = module
            .symbols
            .iter()
            .filter(|s| s.visibility == Visibility::Public)
            .collect();

        writeln!(buf, "### `{}`", module.path).unwrap();
        writeln!(buf).unwrap();
        writeln!(
            buf,
            "Language: **{}** | Symbols: {} | Public API: {}",
            module.language,
            module.symbols.len(),
            public_symbols.len(),
        )
        .unwrap();
        writeln!(buf).unwrap();

        if !public_symbols.is_empty() {
            for sym in &public_symbols {
                writeln!(buf, "- `{}`", sym.signature).unwrap();
            }
        } else {
            writeln!(buf, "_No public symbols._").unwrap();
        }
        writeln!(buf).unwrap();
    }

    // Dependency graph
    writeln!(buf, "---").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "## Dependency Graph").unwrap();
    writeln!(buf).unwrap();
    for module in modules {
        if module.imports.is_empty() {
            continue;
        }
        writeln!(buf, "**`{}`** imports:", module.path).unwrap();
        for import in &module.imports {
            writeln!(buf, "- `{}` ({})", import.source, import.kind).unwrap();
        }
        writeln!(buf).unwrap();
    }

    // Design patterns from intent layer
    writeln!(buf, "---").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "## Design Patterns").unwrap();
    writeln!(buf).unwrap();
    match intent {
        Some(intent_data) if !intent_data.patterns.patterns.is_empty() => {
            writeln!(
                buf,
                "_Generated by {} on {}_",
                intent_data.patterns.generator_model,
                intent_data.patterns.generated_at,
            )
            .unwrap();
            writeln!(buf).unwrap();
            for pattern in &intent_data.patterns.patterns {
                writeln!(buf, "### {} (confidence: {:.2})", pattern.name, pattern.confidence)
                    .unwrap();
                writeln!(buf).unwrap();
                writeln!(buf, "{}", pattern.description).unwrap();
                if !pattern.applies_to.is_empty() {
                    writeln!(buf).unwrap();
                    writeln!(buf, "Applies to:").unwrap();
                    for path in &pattern.applies_to {
                        writeln!(buf, "- `{}`", path).unwrap();
                    }
                }
                writeln!(buf).unwrap();
            }
        }
        Some(_) => {
            writeln!(buf, "_Intent layer generated but no patterns found._").unwrap();
        }
        None => {
            writeln!(buf, "_Intent layer not generated._").unwrap();
        }
    }

    buf
}

// ── L3 — full detail ──────────────────────────────────────────────────────────

/// Generates the L3 markdown: complete repository detail for Opus-class models.
///
/// Extends L2 with every symbol (not just public), full design decisions with
/// rationale + confidence, and the full import/export manifest per module.
///
/// In Scala: `def generateL3(tree: TreeStructure, modules: Seq[ModuleIndex], version: TreeVersion, intent: Option[IntentOutput]): String`
pub fn generate_l3(
    tree: &TreeStructure,
    modules: &[ModuleIndex],
    version: &TreeVersion,
    intent: Option<&IntentOutput>,
) -> String {
    let mut buf = generate_l2(tree, modules, version, intent);

    // All symbols — every symbol across every module, with full metadata
    writeln!(buf).unwrap();
    writeln!(buf, "---").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "## All Symbols").unwrap();
    writeln!(buf).unwrap();
    for module in modules {
        writeln!(buf, "### `{}`", module.path).unwrap();
        writeln!(buf).unwrap();
        if module.symbols.is_empty() {
            writeln!(buf, "_No symbols._").unwrap();
            writeln!(buf).unwrap();
            continue;
        }
        for sym in &module.symbols {
            // Visibility badge — gives readers an at-a-glance access level indicator
            let vis = match sym.visibility {
                Visibility::Public => "pub",
                Visibility::PublicCrate => "pub(crate)",
                Visibility::PublicSuper => "pub(super)",
                Visibility::Private => "priv",
            };
            write!(buf, "- `{}` [{}, {:?}]", sym.signature, vis, sym.kind).unwrap();
            // Span
            write!(buf, " lines {}-{}", sym.span.start_line, sym.span.end_line).unwrap();
            // Parent context
            if let Some(parent) = &sym.parent {
                write!(buf, " in `{}`", parent).unwrap();
            }
            writeln!(buf).unwrap();
            // Doc comment indented under the symbol
            if let Some(doc) = &sym.doc_comment {
                for line in doc.lines() {
                    writeln!(buf, "  > {}", line).unwrap();
                }
            }
        }
        writeln!(buf).unwrap();
    }

    // Design decisions from intent layer
    writeln!(buf, "---").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "## Design Decisions").unwrap();
    writeln!(buf).unwrap();
    match intent {
        Some(intent_data) if !intent_data.decisions.decisions.is_empty() => {
            writeln!(
                buf,
                "_Generated by {} on {}_",
                intent_data.decisions.generator_model,
                intent_data.decisions.generated_at,
            )
            .unwrap();
            writeln!(buf).unwrap();
            for decision in &intent_data.decisions.decisions {
                writeln!(buf, "### {} — {}", decision.id, decision.summary).unwrap();
                writeln!(buf).unwrap();

                // Anchor
                write!(buf, "**Anchor:** `{}`", decision.anchored_to.file).unwrap();
                if let Some(sym) = &decision.anchored_to.symbol {
                    write!(buf, " :: `{}`", sym).unwrap();
                }
                writeln!(buf).unwrap();
                writeln!(buf).unwrap();

                writeln!(buf, "**Rationale:** {}", decision.rationale).unwrap();
                writeln!(buf).unwrap();

                writeln!(
                    buf,
                    "**Confidence:** {:.2} | **Provenance:** {:?}",
                    decision.confidence, decision.provenance,
                )
                .unwrap();
                writeln!(buf).unwrap();
            }
        }
        Some(_) => {
            writeln!(buf, "_Intent layer generated but no decisions found._").unwrap();
        }
        None => {
            writeln!(buf, "_Intent layer not generated._").unwrap();
        }
    }

    // Import/export detail — exhaustive list per module
    writeln!(buf, "---").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "## Import/Export Detail").unwrap();
    writeln!(buf).unwrap();
    for module in modules {
        writeln!(buf, "### `{}`", module.path).unwrap();
        writeln!(buf).unwrap();

        if !module.imports.is_empty() {
            writeln!(buf, "**Imports:**").unwrap();
            for import in &module.imports {
                writeln!(buf, "- `{}` ({})", import.source, import.kind).unwrap();
            }
            writeln!(buf).unwrap();
        }

        if !module.exports.is_empty() {
            writeln!(buf, "**Exports:**").unwrap();
            for export in &module.exports {
                writeln!(buf, "- `{}` ({:?})", export.name, export.kind).unwrap();
            }
            writeln!(buf).unwrap();
        }

        if module.imports.is_empty() && module.exports.is_empty() {
            writeln!(buf, "_No imports or exports._").unwrap();
            writeln!(buf).unwrap();
        }
    }

    buf
}

// ── File writer ───────────────────────────────────────────────────────────────

/// Writes the three Claude layer files to `<output_dir>/claude/`.
///
/// Creates the `claude/` subdirectory if it does not exist, then writes
/// `l1.md`, `l2.md`, and `l3.md`. This is the only function in this module
/// with I/O side effects — everything else is pure string generation.
///
/// In Scala terms: analogous to `IO { Files.write(...) }` in cats-effect.
pub fn write_claude_layer(
    output_dir: &Path,
    l1: &str,
    l2: &str,
    l3: &str,
) -> std::io::Result<()> {
    let claude_dir = output_dir.join("claude");
    fs::create_dir_all(&claude_dir)?;
    fs::write(claude_dir.join("l1.md"), l1)?;
    fs::write(claude_dir.join("l2.md"), l2)?;
    fs::write(claude_dir.join("l3.md"), l3)?;
    Ok(())
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Extracts the final path component of the repo root as the project name.
///
/// E.g. `/home/user/projects/codex-tree` → `"codex-tree"`.
/// Falls back to the full path string if there is no final component
/// (unlikely for any real repo root).
fn project_name_from_root(root: &str) -> &str {
    root.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or(root)
}

/// Returns `TreeEntry::Directory` entries whose path contains no `/` separator
/// after the root — i.e. direct children of the repo root.
///
/// Depth 1 is defined as: the path contains exactly one component (no `/`).
/// For example `"crates"` is depth 1; `"crates/codex-parser"` is depth 2.
fn top_level_dirs(tree: &TreeStructure) -> Vec<&TreeEntry> {
    tree.entries
        .iter()
        .filter(|e| {
            matches!(e, TreeEntry::Directory { path, .. } if !path.contains('/'))
        })
        .collect()
}

/// Collects file paths that define a `main` function.
///
/// A module is an entry point if it contains a symbol whose `kind` is
/// `Function` and whose `name` is `"main"`.
fn find_entry_points(modules: &[ModuleIndex]) -> Vec<&str> {
    modules
        .iter()
        .filter(|m| {
            m.symbols
                .iter()
                .any(|s| s.kind == SymbolKind::Function && s.name == "main")
        })
        .map(|m| m.path.as_str())
        .collect()
}

/// Collects up to `limit` key public types (structs, traits, enums).
///
/// Sorting strategy mirrors the task spec:
///   1. Symbols with a doc comment come first (more informative to the reader).
///   2. Within each group the original order is preserved.
///
/// Returns `(module_path, signature, Option<doc_comment>)` tuples.
fn collect_key_public_types<'a>(
    modules: &'a [ModuleIndex],
    limit: usize,
) -> Vec<(&'a str, &'a str, Option<&'a str>)> {
    let is_key_kind = |kind: &SymbolKind| {
        matches!(kind, SymbolKind::Struct | SymbolKind::Trait | SymbolKind::Enum)
    };

    let mut with_docs: Vec<(&str, &str, Option<&str>)> = Vec::new();
    let mut without_docs: Vec<(&str, &str, Option<&str>)> = Vec::new();

    for module in modules {
        for sym in &module.symbols {
            if sym.visibility == Visibility::Public && is_key_kind(&sym.kind) {
                let entry = (
                    module.path.as_str(),
                    sym.signature.as_str(),
                    sym.doc_comment.as_deref(),
                );
                if sym.doc_comment.is_some() {
                    with_docs.push(entry);
                } else {
                    without_docs.push(entry);
                }
            }
        }
    }

    // Doc-commented types take priority; fill remaining slots from undocumented ones
    with_docs.extend(without_docs);
    with_docs.truncate(limit);
    with_docs
}

/// Extracts the first sentence from a doc comment (up to the first `.` or newline).
///
/// Strips leading `/// ` or `/** */` markers before extracting.
fn first_sentence(doc: &str) -> &str {
    let trimmed = doc.trim_start_matches('/').trim_start_matches('*').trim();
    // Find the end of the first sentence — period, or end of first line
    let end = trimmed
        .find(|c| c == '.' || c == '\n')
        .map(|i| i + 1) // include the period itself
        .unwrap_or(trimmed.len());
    &trimmed[..end]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use codex_parser::types::{
        Export, Import, ModuleIndex, Span, Symbol, SymbolKind, TreeEntry, TreeStats, TreeStructure,
        TreeVersion, TreeVersionNumber, Visibility,
    };

    fn make_version() -> TreeVersion {
        TreeVersion {
            format_version: "1.0.0".to_string(),
            tree_version: TreeVersionNumber {
                generation: 3,
                delta_count: 7,
            },
            generated_at: "2026-03-28T10:00:00Z".to_string(),
            generator: "codex-tree 0.1.0".to_string(),
            source_commit: Some("abc123".to_string()),
            source_commit_date: Some("2026-03-28T09:00:00Z".to_string()),
            stats: TreeStats {
                total_files: 12,
                total_symbols: 87,
                total_lines_of_code: 3_400,
                languages: vec!["rust".to_string(), "toml".to_string()],
                delta_size_bytes: 1_024,
            },
        }
    }

    fn make_tree() -> TreeStructure {
        TreeStructure {
            root: "/home/user/projects/codex-tree".to_string(),
            entries: vec![
                TreeEntry::Directory {
                    path: "crates".to_string(),
                    children_count: 3,
                    total_symbol_count: 87,
                    total_line_count: 3_400,
                },
                TreeEntry::Directory {
                    path: "crates/codex-parser".to_string(),
                    children_count: 2,
                    total_symbol_count: 40,
                    total_line_count: 1_200,
                },
                TreeEntry::File {
                    path: "Cargo.toml".to_string(),
                    language: "toml".to_string(),
                    symbol_count: 0,
                    line_count: 20,
                    imports_count: 0,
                    exports_count: 0,
                },
            ],
        }
    }

    fn make_modules() -> Vec<ModuleIndex> {
        vec![ModuleIndex {
            path: "crates/codex-parser/src/lib.rs".to_string(),
            language: "rust".to_string(),
            content_hash: "deadbeef".to_string(),
            symbols: vec![
                Symbol {
                    name: "main".to_string(),
                    kind: SymbolKind::Function,
                    visibility: Visibility::Public,
                    span: Span { start_line: 1, end_line: 5 },
                    signature: "pub fn main()".to_string(),
                    parent: None,
                    doc_comment: None,
                },
                Symbol {
                    name: "TreeStructure".to_string(),
                    kind: SymbolKind::Struct,
                    visibility: Visibility::Public,
                    span: Span { start_line: 10, end_line: 20 },
                    signature: "pub struct TreeStructure".to_string(),
                    parent: None,
                    doc_comment: Some("The root tree.json document.".to_string()),
                },
            ],
            imports: vec![Import {
                source: "serde::Serialize".to_string(),
                kind: "use".to_string(),
            }],
            exports: vec![Export {
                name: "TreeStructure".to_string(),
                kind: SymbolKind::Struct,
            }],
        }]
    }

    #[test]
    fn metadata_header_format() {
        let v = make_version();
        let header = generate_metadata_header(&v);
        assert_eq!(
            header,
            "<!-- codex-tree: generation=3, delta_count=7, format_version=1.0.0 -->"
        );
    }

    #[test]
    fn l1_contains_key_sections() {
        let l1 = generate_l1(&make_tree(), &make_modules(), &make_version());
        assert!(l1.contains("# Project Overview"));
        assert!(l1.contains("## Quick Stats"));
        assert!(l1.contains("## Architecture"));
        assert!(l1.contains("## Entry Points"));
        assert!(l1.contains("## Key Public Types"));
        // Project name extracted from root
        assert!(l1.contains("codex-tree"));
        // Architecture section should list only depth-1 directories ("crates"),
        // not nested paths like "crates/codex-parser".
        // We check that the bold name "**crates**" appears (the Architecture fmt),
        // while "**crates/codex-parser**" does not — module paths appearing in the
        // Key Public Types section are acceptable and expected.
        assert!(l1.contains("**crates**"));
        assert!(!l1.contains("**crates/codex-parser**"));
    }

    #[test]
    fn l2_extends_l1() {
        let l2 = generate_l2(&make_tree(), &make_modules(), &make_version(), None);
        assert!(l2.contains("## Module Details"));
        assert!(l2.contains("## Dependency Graph"));
        assert!(l2.contains("## Design Patterns"));
        assert!(l2.contains("_Intent layer not generated._"));
        // Must still include L1 content
        assert!(l2.contains("# Project Overview"));
    }

    #[test]
    fn l3_extends_l2() {
        let l3 = generate_l3(&make_tree(), &make_modules(), &make_version(), None);
        assert!(l3.contains("## All Symbols"));
        assert!(l3.contains("## Design Decisions"));
        assert!(l3.contains("## Import/Export Detail"));
        // Must still include L1 + L2 content
        assert!(l3.contains("## Module Details"));
        assert!(l3.contains("# Project Overview"));
    }

    #[test]
    fn write_claude_layer_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        let l1 = "l1 content";
        let l2 = "l2 content";
        let l3 = "l3 content";
        write_claude_layer(dir.path(), l1, l2, l3).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("claude/l1.md")).unwrap(), l1);
        assert_eq!(fs::read_to_string(dir.path().join("claude/l2.md")).unwrap(), l2);
        assert_eq!(fs::read_to_string(dir.path().join("claude/l3.md")).unwrap(), l3);
    }

    #[test]
    fn first_sentence_extraction() {
        assert_eq!(first_sentence("/// Returns the root."), "Returns the root.");
        assert_eq!(first_sentence("Short"), "Short");
        assert_eq!(first_sentence("Line one\nLine two"), "Line one\n");
    }
}
