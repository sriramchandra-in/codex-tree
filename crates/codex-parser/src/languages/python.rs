use tree_sitter::Language;

use crate::language::LanguageAdapter;
use crate::types::{Import, Span, Symbol, SymbolKind, Visibility};

/// Language adapter for Python source files.
pub struct PythonAdapter;

impl LanguageAdapter for PythonAdapter {
    fn language_id(&self) -> &str {
        "python"
    }

    fn file_extensions(&self) -> &[&str] {
        &["py"]
    }

    fn tree_sitter_language(&self) -> Language {
        tree_sitter_python::LANGUAGE.into()
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

impl PythonAdapter {
    fn walk_for_imports(node: tree_sitter::Node, source: &[u8], imports: &mut Vec<Import>) {
        match node.kind() {
            // `import os` or `import os, sys`
            "import_statement" => {
                if let Ok(text) = node.utf8_text(source) {
                    let path = text.trim_start_matches("import").trim();
                    imports.push(Import {
                        source: path.to_string(),
                        kind: "import".to_string(),
                    });
                }
            }
            // `from os.path import join, exists`
            "import_from_statement" => {
                if let Ok(text) = node.utf8_text(source) {
                    imports.push(Import {
                        source: text.to_string(),
                        kind: "from_import".to_string(),
                    });
                }
            }
            _ => {}
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                Self::walk_for_imports(child, source, imports);
            }
        }
    }

    fn map_node_kind(kind: &str) -> Option<SymbolKind> {
        match kind {
            "function_definition" => Some(SymbolKind::Function),
            "class_definition" => Some(SymbolKind::Class),
            _ => None,
        }
    }

    fn walk_for_symbols(
        node: tree_sitter::Node,
        source: &[u8],
        parent: Option<&str>,
        symbols: &mut Vec<Symbol>,
    ) {
        let kind_opt = Self::map_node_kind(node.kind());

        // Track class name so methods can record their parent
        let class_name: Option<String> = if node.kind() == "class_definition" {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string())
        } else {
            None
        };

        if let Some(kind) = kind_opt {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("")
                .to_string();

            // Python uses _ prefix convention for private, __ for name-mangled
            let visibility = if name.starts_with("__") && !name.ends_with("__") {
                Visibility::Private
            } else if name.starts_with('_') {
                Visibility::Private
            } else {
                Visibility::Public
            };

            let span = Span {
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
            };

            // Signature: everything up to the colon (Python uses `:` instead of `{`)
            let node_bytes = &source[node.start_byte()..node.end_byte()];
            let signature = if let Some(colon_offset) = node_bytes.iter().position(|&b| b == b':') {
                std::str::from_utf8(&node_bytes[..colon_offset])
                    .unwrap_or("")
                    .trim()
                    .to_string()
            } else {
                std::str::from_utf8(node_bytes)
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };

            // Docstring: first child of the body that is a string expression
            let doc_comment = node
                .child_by_field_name("body")
                .and_then(|body| body.child(0))
                .filter(|c| c.kind() == "expression_statement")
                .and_then(|expr| expr.child(0))
                .filter(|c| c.kind() == "string")
                .and_then(|s| s.utf8_text(source).ok())
                .map(|s| s.to_string());

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

        // Recurse — items inside a class get the class name as parent
        let child_parent = class_name.as_deref().or(parent);
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                Self::walk_for_symbols(child, source, child_parent, symbols);
            }
        }
    }
}
