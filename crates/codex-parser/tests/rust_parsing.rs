use codex_parser::parser::parse_file;
use codex_parser::registry::ParserRegistry;
use codex_parser::types::{SymbolKind, Visibility};
use std::path::Path;

fn registry() -> ParserRegistry {
    ParserRegistry::with_defaults()
}

// ── Helper ─────────────────────────────────────────────────────────────────────

/// Parse `source` bytes as a `fake.rs` file and return the `ModuleIndex`.
fn parse_rs(source: &[u8]) -> codex_parser::types::ModuleIndex {
    let registry = registry();
    parse_file(Path::new("fake.rs"), source, &registry)
        .expect("parse_file should succeed for valid Rust source")
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[test]
fn test_parse_function() {
    let source =
        b"pub fn hello_world(name: &str) -> String {\n    format!(\"Hello, {}!\", name)\n}\n";

    let module = parse_rs(source);

    assert_eq!(module.symbols.len(), 1, "expected exactly one symbol");
    let sym = &module.symbols[0];
    assert_eq!(sym.name, "hello_world");
    assert_eq!(sym.kind, SymbolKind::Function);
    assert_eq!(sym.visibility, Visibility::Public);
    assert!(
        sym.signature.contains("pub fn hello_world"),
        "signature should contain 'pub fn hello_world', got: {:?}",
        sym.signature
    );
}

#[test]
fn test_parse_struct() {
    let source =
        b"/// A person with a name and age.\npub struct Person {\n    pub name: String,\n    age: u32,\n}\n";

    let module = parse_rs(source);

    let person = module
        .symbols
        .iter()
        .find(|s| s.name == "Person")
        .expect("expected a 'Person' symbol");
    assert_eq!(person.kind, SymbolKind::Struct);
    assert_eq!(person.visibility, Visibility::Public);

    let doc = person
        .doc_comment
        .as_deref()
        .expect("expected a doc comment on Person");
    assert!(
        doc.contains("A person"),
        "doc comment should contain 'A person', got: {:?}",
        doc
    );
}

#[test]
fn test_parse_enum() {
    let source = b"pub enum Color {\n    Red,\n    Green,\n    Blue,\n}\n";

    let module = parse_rs(source);

    let color = module
        .symbols
        .iter()
        .find(|s| s.name == "Color")
        .expect("expected a 'Color' symbol");
    assert_eq!(color.kind, SymbolKind::Enum);
    assert_eq!(color.visibility, Visibility::Public);
}

#[test]
fn test_parse_trait() {
    let source = b"pub trait Greet {\n    fn greet(&self) -> String;\n}\n";

    let module = parse_rs(source);

    let greet = module
        .symbols
        .iter()
        .find(|s| s.name == "Greet")
        .expect("expected a 'Greet' symbol");
    assert_eq!(greet.kind, SymbolKind::Trait);
    assert_eq!(greet.visibility, Visibility::Public);
}

#[test]
fn test_parse_impl_block() {
    let source = b"struct Foo;\n\nimpl Foo {\n    pub fn new() -> Self {\n        Foo\n    }\n\n    fn private_method(&self) {\n    }\n}\n";

    let module = parse_rs(source);

    // The struct Foo should be present.
    let foo_struct = module
        .symbols
        .iter()
        .find(|s| s.name == "Foo" && s.kind == SymbolKind::Struct)
        .expect("expected a 'Foo' struct symbol");
    assert_eq!(foo_struct.visibility, Visibility::Private);

    // The impl block itself should be present.
    let impl_sym = module
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Impl)
        .expect("expected an Impl symbol");
    assert_eq!(impl_sym.name, "Foo");

    // Both methods should be present with parent set to "Foo".
    let new_method = module
        .symbols
        .iter()
        .find(|s| s.name == "new" && s.kind == SymbolKind::Function)
        .expect("expected a 'new' function symbol");
    assert_eq!(
        new_method.parent.as_deref(),
        Some("Foo"),
        "new() should have parent='Foo'"
    );

    let private_method = module
        .symbols
        .iter()
        .find(|s| s.name == "private_method" && s.kind == SymbolKind::Function)
        .expect("expected a 'private_method' function symbol");
    assert_eq!(
        private_method.parent.as_deref(),
        Some("Foo"),
        "private_method() should have parent='Foo'"
    );
}

#[test]
fn test_parse_imports() {
    let source =
        b"use std::collections::HashMap;\nuse crate::types::{Symbol, Import};\n\nfn main() {}\n";

    let module = parse_rs(source);

    assert_eq!(
        module.imports.len(),
        2,
        "expected exactly 2 imports, got: {:?}",
        module.imports
    );

    let sources: Vec<&str> = module.imports.iter().map(|i| i.source.as_str()).collect();

    assert!(
        sources.iter().any(|s| s.contains("std::collections::HashMap")),
        "expected 'std::collections::HashMap' import, got: {:?}",
        sources
    );
    assert!(
        sources
            .iter()
            .any(|s| s.contains("crate::types::{Symbol, Import}")),
        "expected 'crate::types::{{Symbol, Import}}' import, got: {:?}",
        sources
    );
}

#[test]
fn test_parse_multiple_items() {
    let source = b"pub const MAX_SIZE: usize = 100;\n\npub(crate) static COUNTER: u32 = 0;\n\nmod internal {\n    pub fn helper() {}\n}\n\npub type Result<T> = std::result::Result<T, Error>;\n";

    let module = parse_rs(source);

    // We expect at least 4 top-level symbols: const, static, mod, type alias.
    assert!(
        module.symbols.len() >= 4,
        "expected at least 4 symbols, got {}: {:?}",
        module.symbols.len(),
        module.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    let const_sym = module
        .symbols
        .iter()
        .find(|s| s.name == "MAX_SIZE")
        .expect("expected MAX_SIZE symbol");
    assert_eq!(const_sym.kind, SymbolKind::Const);
    assert_eq!(const_sym.visibility, Visibility::Public);

    let static_sym = module
        .symbols
        .iter()
        .find(|s| s.name == "COUNTER")
        .expect("expected COUNTER symbol");
    assert_eq!(static_sym.kind, SymbolKind::Static);
    assert_eq!(static_sym.visibility, Visibility::PublicCrate);

    let mod_sym = module
        .symbols
        .iter()
        .find(|s| s.name == "internal")
        .expect("expected 'internal' module symbol");
    assert_eq!(mod_sym.kind, SymbolKind::Module);
    assert_eq!(mod_sym.visibility, Visibility::Private);

    let type_sym = module
        .symbols
        .iter()
        .find(|s| s.name == "Result")
        .expect("expected 'Result' type alias symbol");
    assert_eq!(type_sym.kind, SymbolKind::TypeAlias);
    assert_eq!(type_sym.visibility, Visibility::Public);
}

#[test]
fn test_parse_visibility_variants() {
    let source = b"pub fn public_fn() {}\npub(crate) fn crate_fn() {}\npub(super) fn super_fn() {}\nfn private_fn() {}\n";

    let module = parse_rs(source);

    // We should have exactly 4 function symbols.
    let functions: Vec<_> = module
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Function)
        .collect();
    assert_eq!(
        functions.len(),
        4,
        "expected 4 function symbols, got: {:?}",
        functions.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    let find = |name: &str| {
        module
            .symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("expected symbol '{}'", name))
    };

    assert_eq!(find("public_fn").visibility, Visibility::Public);
    assert_eq!(find("crate_fn").visibility, Visibility::PublicCrate);
    assert_eq!(find("super_fn").visibility, Visibility::PublicSuper);
    assert_eq!(find("private_fn").visibility, Visibility::Private);
}
