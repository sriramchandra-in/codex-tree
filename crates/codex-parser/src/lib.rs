// codex-parser: Language-independent AST parser for the codex-tree knowledge tree.
//
// Module layout mirrors a Scala package structure:
//   error    — ParserError enum + Result<T> alias
//   types    — all data model structs / enums (case classes)
//   language — LanguageAdapter trait (abstract interface)
//   languages — concrete adapter implementations (one sub-module per language)
//   registry — ParserRegistry (maps extensions → adapters)

pub mod error;
pub mod language;
pub mod languages;
pub mod registry;
pub mod types;

pub mod parser;
pub mod serializer;
