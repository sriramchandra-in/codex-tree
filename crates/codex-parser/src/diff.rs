/// Structural diff between two `ModuleIndex` objects.
///
/// Produces a `DeltaOperation::Modify` that records which symbols were added,
/// removed, or had their signature / span changed — analogous to a structural
/// diff in a version-controlled schema rather than a line diff.
use std::collections::HashSet;

use crate::types::{DeltaOperation, ModuleIndex};

/// Compare an old and new `ModuleIndex` and return a `DeltaOperation::Modify`
/// that describes what changed at the symbol level.
///
/// - **Added** — names present in `new` but absent in `old`.
/// - **Removed** — names present in `old` but absent in `new`.
/// - **Modified** — names present in both but with a different `signature`,
///   `span.start_line`, or `span.end_line`.
pub fn diff_modules(old: &ModuleIndex, new: &ModuleIndex) -> DeltaOperation {
    let old_symbol_names: HashSet<&str> = old.symbols.iter().map(|s| s.name.as_str()).collect();
    let new_symbol_names: HashSet<&str> = new.symbols.iter().map(|s| s.name.as_str()).collect();

    let mut symbols_added: Vec<String> = new_symbol_names
        .difference(&old_symbol_names)
        .map(|s| s.to_string())
        .collect();
    symbols_added.sort();

    let mut symbols_removed: Vec<String> = old_symbol_names
        .difference(&new_symbol_names)
        .map(|s| s.to_string())
        .collect();
    symbols_removed.sort();

    // Modified = present in both but signature or span changed.
    let mut symbols_modified: Vec<String> = old
        .symbols
        .iter()
        .filter(|old_sym| {
            new.symbols.iter().any(|new_sym| {
                new_sym.name == old_sym.name
                    && (new_sym.signature != old_sym.signature
                        || new_sym.span.start_line != old_sym.span.start_line
                        || new_sym.span.end_line != old_sym.span.end_line)
            })
        })
        .map(|s| s.name.clone())
        .collect();
    symbols_modified.sort();

    DeltaOperation::Modify {
        path: new.path.clone(),
        symbols_added,
        symbols_removed,
        symbols_modified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Export, Import, ModuleIndex, Span, Symbol, SymbolKind, Visibility};

    fn make_symbol(name: &str, sig: &str, start: usize, end: usize) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            span: Span {
                start_line: start,
                end_line: end,
            },
            signature: sig.to_string(),
            parent: None,
            doc_comment: None,
        }
    }

    fn make_module(path: &str, symbols: Vec<Symbol>) -> ModuleIndex {
        ModuleIndex {
            path: path.to_string(),
            language: "rust".to_string(),
            content_hash: "sha256:abc".to_string(),
            symbols,
            imports: Vec::<Import>::new(),
            exports: Vec::<Export>::new(),
        }
    }

    #[test]
    fn test_no_changes() {
        let sym = make_symbol("foo", "pub fn foo()", 1, 5);
        let old = make_module("src/lib.rs", vec![sym.clone()]);
        let new = make_module("src/lib.rs", vec![sym]);

        match diff_modules(&old, &new) {
            DeltaOperation::Modify {
                symbols_added,
                symbols_removed,
                symbols_modified,
                ..
            } => {
                assert!(symbols_added.is_empty());
                assert!(symbols_removed.is_empty());
                assert!(symbols_modified.is_empty());
            }
            _ => panic!("expected Modify"),
        }
    }

    #[test]
    fn test_symbol_added() {
        let old = make_module("src/lib.rs", vec![make_symbol("foo", "pub fn foo()", 1, 5)]);
        let new = make_module(
            "src/lib.rs",
            vec![
                make_symbol("foo", "pub fn foo()", 1, 5),
                make_symbol("bar", "pub fn bar()", 7, 10),
            ],
        );

        match diff_modules(&old, &new) {
            DeltaOperation::Modify { symbols_added, .. } => {
                assert_eq!(symbols_added, vec!["bar"]);
            }
            _ => panic!("expected Modify"),
        }
    }

    #[test]
    fn test_symbol_removed() {
        let old = make_module(
            "src/lib.rs",
            vec![
                make_symbol("foo", "pub fn foo()", 1, 5),
                make_symbol("bar", "pub fn bar()", 7, 10),
            ],
        );
        let new = make_module("src/lib.rs", vec![make_symbol("foo", "pub fn foo()", 1, 5)]);

        match diff_modules(&old, &new) {
            DeltaOperation::Modify {
                symbols_removed, ..
            } => {
                assert_eq!(symbols_removed, vec!["bar"]);
            }
            _ => panic!("expected Modify"),
        }
    }

    #[test]
    fn test_symbol_modified_signature() {
        let old = make_module("src/lib.rs", vec![make_symbol("foo", "pub fn foo()", 1, 5)]);
        let new = make_module(
            "src/lib.rs",
            vec![make_symbol("foo", "pub fn foo(x: i32) -> bool", 1, 5)],
        );

        match diff_modules(&old, &new) {
            DeltaOperation::Modify {
                symbols_modified, ..
            } => {
                assert_eq!(symbols_modified, vec!["foo"]);
            }
            _ => panic!("expected Modify"),
        }
    }
}
