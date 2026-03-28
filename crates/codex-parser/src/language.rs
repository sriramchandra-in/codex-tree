use tree_sitter::Language;

use crate::types::{Import, Symbol};

/// The core abstraction for language support — every language plugin implements this.
///
/// In Scala this would be:
/// ```scala
/// trait LanguageAdapter {
///   def languageId: String
///   def fileExtensions: Seq[String]
///   def extractSymbols(tree: Tree, source: Array[Byte]): Seq[Symbol]
///   def extractImports(tree: Tree, source: Array[Byte]): Seq[Import]
/// }
/// ```
///
/// The `Send + Sync` bounds are the Rust equivalent of "this object is safe to
/// share across threads" — required because the registry may be used from a
/// parallel file-walking task.
pub trait LanguageAdapter: Send + Sync {
    /// Stable, lowercase identifier for this language, e.g. `"rust"` or `"python"`.
    fn language_id(&self) -> &str;

    /// File extensions (without leading dot) handled by this adapter, e.g. `&["rs"]`.
    /// A single adapter may claim multiple extensions (e.g. `&["js", "mjs", "cjs"]`).
    fn file_extensions(&self) -> &[&str];

    /// The compiled tree-sitter grammar.  Called once per parse invocation.
    fn tree_sitter_language(&self) -> Language;

    /// Walk the parsed CST and return every named symbol in the file.
    ///
    /// `source` is the raw UTF-8 bytes of the file — tree-sitter node methods
    /// like `utf8_text` need it to materialise string slices without copying.
    fn extract_symbols(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Vec<Symbol>;

    /// Walk the parsed CST and return all import / dependency declarations.
    fn extract_imports(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Vec<Import>;
}
