/// Analyzer error types — analogous to a Scala sealed trait hierarchy with case classes.
///
/// `thiserror` generates the `From` impls and `Display` messages automatically,
/// so you never write boilerplate conversion code by hand.
///
/// Error messages in this crate follow the project's "teaching pattern":
///   1. What went wrong
///   2. Why it matters
///   3. How to fix it
///   4. An AI hint for CI / automated contexts
#[derive(Debug, thiserror::Error)]
pub enum AnalyzerError {
    /// Wraps `reqwest::Error`. The `#[from]` attribute means Rust will automatically
    /// convert reqwest::Error -> AnalyzerError via the `?` operator — like Scala's
    /// implicit conversion but explicit and zero-cost.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Claude API key not set.\n\nThe intent and Claude layers require an API key.\n\n  To fix: export ANTHROPIC_API_KEY=sk-ant-...\n  To skip: use --no-intent --no-claude\n\n[AI hint: In CI, set ANTHROPIC_API_KEY as a repository secret.]")]
    ApiKeyMissing,

    /// Struct variant — carries named fields rather than a positional tuple.
    /// In Scala: `case class ApiError(status: Int, body: String) extends AnalyzerError`.
    #[error("Claude API error (HTTP {status}): {body}\n\nThis may be a transient error. Retry, or use --no-intent to skip.")]
    ApiError { status: u16, body: String },

    #[error("Failed to parse Claude API response: {0}\n\nThe model may have returned unexpected output. Try re-running.")]
    ResponseParse(String),

    /// Struct variant with named fields for budget accounting.
    #[error("Token budget exceeded: used {used} of {limit} allowed.\n\nIncrease CODEX_TREE_BUDGET or use --no-intent to skip AI analysis.")]
    BudgetExceeded { used: usize, limit: usize },

    /// Wraps std::io::Error via `#[from]` — the `?` operator auto-converts.
    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// Wraps serde_json::Error for JSON serialisation/deserialisation failures.
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

/// Convenience alias — mirrors Scala's `type Result[A] = Either[AnalyzerError, A]`
/// but using Rust's built-in `Result<T, E>`. Import this in every module so
/// you can write `Result<Foo>` instead of `std::result::Result<Foo, AnalyzerError>`.
pub type Result<T> = std::result::Result<T, AnalyzerError>;
