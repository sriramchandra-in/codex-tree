/// Parser error types — analogous to a Scala sealed trait hierarchy with case classes.
///
/// `thiserror` generates the `From` impls and `Display` messages automatically,
/// so you never write boilerplate conversion code by hand.
#[derive(Debug, thiserror::Error)]
pub enum ParserError {
    /// Wraps std::io::Error. The `#[from]` attribute means Rust will automatically
    /// convert io::Error -> ParserError via the `?` operator — like Scala's
    /// implicit conversion but explicit and zero-cost.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Returned when tree-sitter fails to produce a valid tree. Contains a
    /// human-readable description of what went wrong during parsing.
    #[error("tree-sitter parse failure: {0}")]
    TreeSitterError(String),

    /// Returned when a file extension has no registered LanguageAdapter.
    /// The String contains the unrecognised extension (e.g. "rb", "zig").
    #[error("no language adapter registered for extension: '{0}'")]
    UnsupportedLanguage(String),

    /// Wraps serde_json::Error for JSON serialisation/deserialisation failures.
    #[error("JSON serialisation error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Returned when a git operation fails (e.g. resolving HEAD commit).
    #[error("git operation failed: {0}")]
    GitError(String),

    /// Returned when a tree file exists but its structure is invalid or
    /// inconsistent (e.g. a file entry references a non-existent parent).
    #[error("invalid tree structure: {0}")]
    InvalidTree(String),
}

/// Convenience alias — mirrors Scala's `type Result[A] = Either[ParserError, A]`
/// but using Rust's built-in `Result<T, E>`.  Import this in every module so
/// you can write `Result<Foo>` instead of `std::result::Result<Foo, ParserError>`.
pub type Result<T> = std::result::Result<T, ParserError>;
