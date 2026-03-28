use codex_parser::parser::parse_directory;
use codex_parser::registry::ParserRegistry;
use std::path::Path;

// ── Self-parse tests ───────────────────────────────────────────────────────────

/// Parse the codex-parser crate's own `src/` directory.
///
/// This is the primary dogfooding test: it verifies that the parser can
/// successfully walk a real Rust codebase and extract meaningful symbols.
#[test]
fn test_self_parse() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    let registry = ParserRegistry::with_defaults();
    let (tree, modules) = parse_directory(&root, &registry, &[".git", "target"])
        .expect("parse_directory should succeed on own src/");

    // The tree should have at least some entries.
    assert!(
        !tree.entries.is_empty(),
        "TreeStructure.entries should not be empty when parsing codex-parser/src"
    );

    // We should have found at least a few Rust source files.
    assert!(
        !modules.is_empty(),
        "modules list should not be empty when parsing codex-parser/src"
    );

    // All discovered files should be reported as "rust".
    for module in &modules {
        assert_eq!(
            module.language, "rust",
            "all files in src/ should be parsed as 'rust', but '{}' has language '{}'",
            module.path, module.language
        );
    }

    // Collect all symbol names across the codebase.
    let all_symbol_names: Vec<&str> = modules
        .iter()
        .flat_map(|m| m.symbols.iter().map(|s| s.name.as_str()))
        .collect();

    // We expect to find these well-known symbols from the production code.
    let expected_symbols = ["ParserRegistry", "LanguageAdapter", "Symbol"];
    for expected in &expected_symbols {
        assert!(
            all_symbol_names.contains(expected),
            "expected to find symbol '{}' in the parsed output, but it was not found. \
             All symbols: {:?}",
            expected,
            all_symbol_names
        );
    }

    // Summary — useful for debugging when running `cargo test -- --nocapture`.
    let total_symbols: usize = modules.iter().map(|m| m.symbols.len()).sum();
    println!(
        "[test_self_parse] parsed {} file(s), {} symbol(s) total",
        modules.len(),
        total_symbols
    );
    println!("[test_self_parse] tree entries: {}", tree.entries.len());
}

/// Parse the full codex-tree workspace root.
///
/// This is the broader dogfooding test — it exercises the directory walker
/// across multiple crates and ensures no panic or hard error occurs.
#[test]
fn test_self_parse_full_repo() {
    // CARGO_MANIFEST_DIR is .../crates/codex-parser; go up two levels to reach
    // the workspace root.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/ directory should exist")
        .parent()
        .expect("workspace root should exist");

    let registry = ParserRegistry::with_defaults();
    let (tree, modules) = parse_directory(
        root,
        &registry,
        &[".git", "target", ".codex-tree", "node_modules"],
    )
    .expect("parse_directory should succeed on workspace root");

    // The tree must have entries.
    assert!(
        !tree.entries.is_empty(),
        "TreeStructure.entries should not be empty for the workspace root"
    );

    // We should find multiple files.
    assert!(
        modules.len() >= 2,
        "expected at least 2 parsed modules in the workspace, got {}",
        modules.len()
    );

    let total_symbols: usize = modules.iter().map(|m| m.symbols.len()).sum();
    println!(
        "[test_self_parse_full_repo] parsed {} file(s), {} symbol(s) total",
        modules.len(),
        total_symbols
    );
    println!(
        "[test_self_parse_full_repo] tree entries: {}",
        tree.entries.len()
    );
}
