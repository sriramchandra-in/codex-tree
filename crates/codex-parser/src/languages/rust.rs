use tree_sitter::Language;

use crate::language::LanguageAdapter;
use crate::types::{Import, Span, Symbol, SymbolKind, Visibility};

/// Language adapter for Rust source files.
///
/// This struct is a unit struct (no fields) — equivalent to a Scala `object`
/// that extends a trait.  It exists only as a handle to call the trait methods.
pub struct RustAdapter;

impl LanguageAdapter for RustAdapter {
    fn language_id(&self) -> &str {
        "rust"
    }

    fn file_extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn tree_sitter_language(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn extract_symbols(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        Self::walk_for_symbols(tree.root_node(), source, None, &mut symbols);
        symbols
    }

    fn extract_imports(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Vec<Import> {
        let mut imports = Vec::new();
        Self::walk_for_imports(tree.root_node(), source, &mut imports);
        imports
    }
}

impl RustAdapter {
    /// Recursively walk the CST collecting `use_declaration` nodes.
    ///
    /// This is a simple recursive descent — for very large files a cursor-based
    /// iterative walk would be more stack-efficient, but this is fine for now.
    fn walk_for_imports(node: tree_sitter::Node, source: &[u8], imports: &mut Vec<Import>) {
        if node.kind() == "use_declaration" {
            if let Ok(text) = node.utf8_text(source) {
                imports.push(Import {
                    // Strip the leading "use " and trailing ";" to get a clean path.
                    source: text
                        .trim_start_matches("use ")
                        .trim_end_matches(';')
                        .trim()
                        .to_string(),
                    kind: "use".to_string(),
                });
            }
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                Self::walk_for_imports(child, source, imports);
            }
        }
    }

    /// Recursively walk the CST collecting declaration nodes as `Symbol`s.
    ///
    /// `parent` carries the name of the enclosing `impl` block so that methods
    /// and associated items can record their owner.
    fn walk_for_symbols(
        node: tree_sitter::Node,
        source: &[u8],
        parent: Option<&str>,
        symbols: &mut Vec<Symbol>,
    ) {
        let kind_opt: Option<SymbolKind> = match node.kind() {
            "function_item" => Some(SymbolKind::Function),
            "struct_item" => Some(SymbolKind::Struct),
            "enum_item" => Some(SymbolKind::Enum),
            "trait_item" => Some(SymbolKind::Trait),
            "impl_item" => Some(SymbolKind::Impl),
            "const_item" => Some(SymbolKind::Const),
            "static_item" => Some(SymbolKind::Static),
            "type_item" => Some(SymbolKind::TypeAlias),
            "mod_item" => Some(SymbolKind::Module),
            "macro_definition" => Some(SymbolKind::Macro),
            _ => None,
        };

        // Determine the impl target name so children can use it as their parent.
        // We compute this regardless of whether this node itself emits a symbol.
        let impl_target: Option<String> = if node.kind() == "impl_item" {
            // The type being implemented is the first `type_identifier` child.
            (0..node.child_count())
                .filter_map(|i| node.child(i))
                .find(|c| c.kind() == "type_identifier" || c.kind() == "generic_type")
                .and_then(|c| c.utf8_text(source).ok())
                .map(|s| s.to_string())
        } else {
            None
        };

        if let Some(kind) = kind_opt {
            // ── Name ────────────────────────────────────────────────────────────
            let name: String = if node.kind() == "impl_item" {
                // For impl blocks use the type target computed above.
                impl_target.clone().unwrap_or_default()
            } else {
                node.child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string()
            };

            // ── Visibility ──────────────────────────────────────────────────────
            let visibility: Visibility = (0..node.child_count())
                .filter_map(|i| node.child(i))
                .find(|c| c.kind() == "visibility_modifier")
                .and_then(|c| c.utf8_text(source).ok())
                .map(|text| match text.trim() {
                    "pub(crate)" => Visibility::PublicCrate,
                    "pub(super)" => Visibility::PublicSuper,
                    _ => Visibility::Public,
                })
                .unwrap_or(Visibility::Private);

            // ── Span (1-indexed) ────────────────────────────────────────────────
            let span = Span {
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
            };

            // ── Signature ───────────────────────────────────────────────────────
            let node_bytes = &source[node.start_byte()..node.end_byte()];
            let signature: String = {
                // Find the first `{` byte in this node's source slice.
                if let Some(brace_offset) = node_bytes.iter().position(|&b| b == b'{') {
                    std::str::from_utf8(&node_bytes[..brace_offset])
                        .unwrap_or("")
                        .trim()
                        .to_string()
                } else if let Some(semi_offset) = node_bytes.iter().position(|&b| b == b';') {
                    std::str::from_utf8(&node_bytes[..semi_offset])
                        .unwrap_or("")
                        .trim()
                        .to_string()
                } else {
                    std::str::from_utf8(node_bytes)
                        .unwrap_or("")
                        .trim()
                        .to_string()
                }
            };

            // ── Doc comment ─────────────────────────────────────────────────────
            let doc_comment: Option<String> = {
                let mut lines: Vec<String> = Vec::new();
                let mut sib = node.prev_sibling();
                while let Some(s) = sib {
                    if s.kind() == "line_comment" {
                        if let Ok(text) = s.utf8_text(source) {
                            if text.starts_with("///") {
                                lines.push(text.to_string());
                                sib = s.prev_sibling();
                                continue;
                            }
                        }
                    }
                    break;
                }
                if lines.is_empty() {
                    None
                } else {
                    lines.reverse();
                    Some(lines.join("\n"))
                }
            };

            symbols.push(Symbol {
                name,
                kind,
                visibility,
                span,
                signature,
                parent: parent.map(|s| s.to_string()),
                doc_comment,
            });
        }

        // ── Recurse into children ────────────────────────────────────────────────
        // Items directly inside an impl block get the impl's type as their parent.
        let child_parent = impl_target.as_deref().or(parent);
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                Self::walk_for_symbols(child, source, child_parent, symbols);
            }
        }
    }
}
