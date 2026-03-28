/// All data model types for codex-tree's knowledge representation.
///
/// In Scala terms: every `struct` here is a case class, every `enum` is a
/// sealed trait with case objects/classes.  `#[derive(Serialize, Deserialize)]`
/// is like mixing in a codec typeclass automatically.
use serde::{Deserialize, Serialize};

// ── Symbol taxonomy ──────────────────────────────────────────────────────────

/// The kind of a named symbol — normalised across all supported languages.
///
/// `#[serde(rename_all = "snake_case")]` serialises `SymbolKind::TypeAlias`
/// as `"type_alias"` in JSON, matching common LSP/tree-sitter conventions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    /// Rust `trait`, Java/TypeScript `interface` (semantic equivalent)
    Trait,
    /// Rust `impl` block — no direct Scala analogue; closest is a companion object
    Impl,
    TypeAlias,
    Const,
    Static,
    Module,
    Macro,
    /// Explicit interface keyword (TypeScript, Go) — distinct from Trait when needed
    Interface,
    Class,
    Method,
    Field,
    Variable,
}

// ── Visibility ────────────────────────────────────────────────────────────────

/// Visibility modifier — normalised across languages.
///
/// Rust has fine-grained path-based visibility; this enum captures the most
/// common cases.  Languages without a visibility concept default to `Public`
/// (e.g. Python, JavaScript top-level exports).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    /// `pub(crate)` — visible within the same crate only
    PublicCrate,
    /// `pub(super)` — visible to the parent module only
    PublicSuper,
    Private,
}

// ── Source location ───────────────────────────────────────────────────────────

/// A half-open line range `[start_line, end_line)` within a source file.
/// Lines are 0-indexed to match tree-sitter's convention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub start_line: usize,
    pub end_line: usize,
}

// ── Core symbol type ──────────────────────────────────────────────────────────

/// A named symbol extracted from the AST.
///
/// `#[serde(skip_serializing_if = "Option::is_none")]` omits `null` fields from
/// JSON output — like a Scala `Option` that serialises as absent rather than null.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub visibility: Visibility,
    pub span: Span,
    /// The declaration line(s), e.g. `pub fn foo(x: i32) -> bool` without the body.
    pub signature: String,
    /// Name of the enclosing symbol, if any (e.g. the `impl` block for a method).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Doc comment immediately preceding the symbol (`///` or `/** */`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
}

// ── Imports / exports ─────────────────────────────────────────────────────────

/// A dependency declaration (`use`, `import`, `require`, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    /// The imported path or module name, e.g. `"std::collections::HashMap"`.
    pub source: String,
    /// The keyword used: `"use"` (Rust), `"import"` (Java/TS), `"require"` (JS).
    pub kind: String,
}

/// A symbol that is part of a module's public API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Export {
    pub name: String,
    pub kind: SymbolKind,
}

// ── Per-file index ────────────────────────────────────────────────────────────

/// The full structural analysis result for a single source file.
///
/// Stored as `<hash-prefix>/module.json` inside the `.codex-tree` delta store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleIndex {
    /// Repo-relative path, e.g. `"src/lib.rs"`.
    pub path: String,
    /// Language ID as returned by `LanguageAdapter::language_id`.
    pub language: String,
    /// SHA-256 (hex) of the file's byte content — used for change detection.
    pub content_hash: String,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
    pub exports: Vec<Export>,
}

// ── Top-level tree entries ────────────────────────────────────────────────────

/// An entry in `tree.json` — either a file or a directory node.
///
/// `#[serde(tag = "type")]` is a *tagged union*: the JSON object carries a
/// `"type": "file"` or `"type": "directory"` field that acts as the discriminant —
/// analogous to writing `"$type"` in Scala's Jackson config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TreeEntry {
    File {
        path: String,
        language: String,
        symbol_count: usize,
        line_count: usize,
        imports_count: usize,
        exports_count: usize,
    },
    Directory {
        path: String,
        children_count: usize,
        total_symbol_count: usize,
        total_line_count: usize,
    },
}

/// The root `tree.json` document that indexes the whole repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeStructure {
    /// Repo root as an absolute path.
    pub root: String,
    pub entries: Vec<TreeEntry>,
}

// ── Version tracking ──────────────────────────────────────────────────────────

/// Content of `version.json` — records when and how the tree was generated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeVersion {
    /// Semver string for the tree format itself (bumped on breaking schema changes).
    pub format_version: String,
    pub tree_version: TreeVersionNumber,
    /// ISO-8601 timestamp of generation, e.g. `"2026-03-27T14:00:00Z"`.
    pub generated_at: String,
    /// Tool + version string, e.g. `"codex-tree 0.1.0"`.
    pub generator: String,
    /// Git commit SHA the tree was built from (absent for non-git directories).
    pub source_commit: Option<String>,
    /// Commit author date in ISO-8601 format.
    pub source_commit_date: Option<String>,
    pub stats: TreeStats,
}

/// A two-part monotonic version number: `generation.delta_count`.
///
/// `generation` increments on a full rebuild; `delta_count` increments on
/// each incremental update within that generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeVersionNumber {
    pub generation: u32,
    pub delta_count: u32,
}

impl std::fmt::Display for TreeVersionNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.generation, self.delta_count)
    }
}

/// Aggregate statistics stored in `version.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeStats {
    pub total_files: usize,
    pub total_symbols: usize,
    pub total_lines_of_code: usize,
    /// Sorted list of language IDs present in the repo, e.g. `["rust", "toml"]`.
    pub languages: Vec<String>,
    /// Compressed size of the delta store in bytes.
    pub delta_size_bytes: u64,
}

// ── Delta types ───────────────────────────────────────────────────────────────

/// A single delta operation on a module.
///
/// `#[serde(tag = "op")]` serialises as `{"op": "add", ...}` — a tagged union
/// analogous to a Scala sealed trait with `@JsonTypeInfo` discriminator field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DeltaOperation {
    Add {
        path: String,
        module_index: ModuleIndex,
    },
    Modify {
        path: String,
        symbols_added: Vec<String>,
        symbols_removed: Vec<String>,
        symbols_modified: Vec<String>,
    },
    Remove {
        path: String,
    },
}

/// An incremental update record stored as `deltas/{sequence}.json`.
///
/// Each delta describes the structural changes between one tree state and
/// the next, indexed by the git commit that triggered the update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    pub sequence: u32,
    pub timestamp: String,
    pub source_commit: Option<String>,
    pub changed_files: Vec<String>,
    pub operations: Vec<DeltaOperation>,
}
